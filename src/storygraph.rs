// HTTP client for the StoryGraph unofficial web API.
//
// StoryGraph has no public API. All interactions mirror what a browser does:
// session-cookie auth, HTML-scraped responses for reads, and HTML form POSTs
// for writes. A CSRF token is extracted from the page HTML before each POST.
//
// Auth cookies come from browser DevTools and live in config.ini:
//   session_cookie      → _storygraph_session
//   remember_user_token → remember_user_token
//
// This is a faithful port of the NickelStoryGraph (Kobo/Tolino) client,
// trimmed to what the Kindle daemon needs: login, search/match, status and
// progress. Goals/streak/feed/journal/reviews/editions are not ported.

use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, Response};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::{Certificate, StatusCode, Url};
use retry::delay::{Exponential, jitter};
use retry::retry;
use scraper::{Html, Selector};
use serde::Serialize;

use crate::config::CONFIG;
use crate::error::UserError;

pub const BASE_URL: &str = "https://app.thestorygraph.com";

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36";

macro_rules! debug_log {
    ($($t:tt)*) => {{
        if crate::config::CONFIG.debug {
            eprintln!("{}", format!($($t)*));
        }
    }};
}

// ── Output types (serialized to JSON by the CLI) ─────────────────────────────

#[derive(Serialize, Debug)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: String,
    pub pages: u32,
}

#[derive(Serialize, Debug)]
pub struct UserBook {
    pub book_id: String,
    pub status_id: Option<u8>,
    pub progress_percent: u32,
    pub progress_pages: u32,
    pub book_pages: u32,
    pub progress_type: String,
    pub edition_format: String,
}

#[derive(Serialize, Debug)]
pub struct User {
    pub username: String,
    pub slug: String,
}

#[derive(Serialize, Debug)]
pub struct LoginResult {
    pub session_cookie: String,
    pub remember_user_token: String,
}

/// StoryGraph's internal reading-status IDs (stable across the site).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ReadingStatus {
    ToRead = 1,
    CurrentlyReading = 2,
    Read = 3,
    Paused = 4,
    DidNotFinish = 5,
    Rereading = 6,
}

impl ReadingStatus {
    pub fn from_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(Self::ToRead),
            2 => Some(Self::CurrentlyReading),
            3 => Some(Self::Read),
            4 => Some(Self::Paused),
            5 => Some(Self::DidNotFinish),
            6 => Some(Self::Rereading),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToRead => "to-read",
            Self::CurrentlyReading => "currently-reading",
            Self::Read => "read",
            Self::Paused => "paused",
            Self::DidNotFinish => "did-not-finish",
            Self::Rereading => "rereading",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let lower = text.to_lowercase();
        if lower.contains("rereading") {
            Some(Self::Rereading)
        } else if lower.contains("currently reading") {
            Some(Self::CurrentlyReading)
        } else if lower.contains("to-read") || lower.contains("to read") {
            Some(Self::ToRead)
        } else if lower.contains("paused") {
            Some(Self::Paused)
        } else if lower.contains("did not finish") {
            Some(Self::DidNotFinish)
        } else if lower.trim() == "read" || lower.ends_with(" read") || lower.starts_with("read ") {
            Some(Self::Read)
        } else {
            None
        }
    }
}

// Page-level state extracted from /books/:slug before any mutating POST.
struct PageData {
    csrf: String,
    book_pages: String,
}

// ── Singleton client ──────────────────────────────────────────────────────────

fn build_client(jar: Arc<Jar>) -> Result<Client, reqwest::Error> {
    Client::builder()
        .cookie_provider(jar)
        .user_agent(USER_AGENT)
        .tls_certs_only(
            webpki_root_certs::TLS_SERVER_ROOT_CERTS
                .iter()
                .map(|cert| Certificate::from_der(cert))
                .collect::<Result<Vec<_>, _>>()?,
        )
        .build()
}

static CLIENT: LazyLock<Result<Client, reqwest::Error>> = LazyLock::new(|| {
    let base: Url = BASE_URL.parse().expect("BASE_URL is valid");
    let jar = Arc::new(Jar::default());

    // Pre-seed auth cookies from config.
    jar.add_cookie_str(
        &format!("_storygraph_session={}", CONFIG.session_cookie),
        &base,
    );
    jar.add_cookie_str(
        &format!("remember_user_token={}", CONFIG.remember_user_token),
        &base,
    );
    jar.add_cookie_str("cookies_popup_seen=yes", &base);
    jar.add_cookie_str("plus_popup_seen=yes", &base);

    build_client(jar)
});

fn client() -> Result<&'static Client> {
    CLIENT.as_ref().context("Failed to construct HTTP client")
}

// ── Guards ────────────────────────────────────────────────────────────────────

fn require_credentials() -> Result<()> {
    if CONFIG.session_cookie.is_empty() {
        return Err(UserError(
            "Please set session_cookie in config.ini. Obtain it from browser DevTools → Application → Cookies → app.thestorygraph.com.".to_owned(),
        )
        .into());
    }
    Ok(())
}

/// Returns Err if the final response URL ended up on the sign-in page.
fn check_session(response: &Response) -> Result<()> {
    if response.url().path().contains("/users/sign_in") {
        bail!(
            "StoryGraph session has expired. Please update session_cookie and remember_user_token in config.ini."
        );
    }
    Ok(())
}

// ── Low-level HTTP ────────────────────────────────────────────────────────────

fn get(path: &str) -> Result<String> {
    let url = format!("{BASE_URL}{path}");
    debug_log!("GET {url}");

    let res = retry(Exponential::from_millis(500).map(jitter).take(3), || {
        client()?
            .get(&url)
            .send()
            .context(format!("GET {url} failed"))
    })
    .map_err(|e| e.error)?;

    check_session(&res)?;
    res.error_for_status_ref()
        .context(format!("GET {url} returned error status"))?;
    res.text()
        .context(format!("Failed to read body from GET {url}"))
}

fn post_form(path: &str, fields: &[(&str, &str)]) -> Result<String> {
    let url = format!("{BASE_URL}{path}");
    debug_log!("POST {url}");

    let csrf = fields
        .iter()
        .find(|(k, _)| *k == "authenticity_token")
        .map(|(_, v)| *v)
        .unwrap_or("");

    let res = retry(Exponential::from_millis(500).map(jitter).take(3), || {
        client()?
            .post(&url)
            .header("X-Requested-With", "XMLHttpRequest")
            .header(
                "Accept",
                "text/javascript, application/javascript, application/ecmascript, */*; q=0.01",
            )
            .header("Origin", BASE_URL)
            .header("Referer", &url)
            .header("X-CSRF-Token", csrf)
            .form(fields)
            .send()
            .context(format!("POST {url} failed"))
    })
    .map_err(|e| e.error)?;

    // 302 redirect back to the book page is a success for StoryGraph mutations.
    if res.status() == StatusCode::FOUND {
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if location.contains("/users/sign_in") {
            bail!(
                "StoryGraph session has expired. Please update session_cookie and remember_user_token in config.ini."
            );
        }
        return Ok(String::new());
    }

    check_session(&res)?;
    res.error_for_status_ref()
        .context(format!("POST {url} returned error status"))?;
    res.text()
        .context(format!("Failed to read body from POST {url}"))
}

// ── HTML helpers ──────────────────────────────────────────────────────────────

fn csrf_token(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);

    let meta_sel = Selector::parse(r#"meta[name="csrf-token"]"#).expect("valid selector");
    if let Some(content) = doc
        .select(&meta_sel)
        .next()
        .and_then(|el| el.value().attr("content"))
    {
        return Some(content.to_owned());
    }

    let input_sel = Selector::parse(r#"input[name="authenticity_token"]"#).expect("valid selector");
    doc.select(&input_sel)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_owned)
}

fn text_of(doc: &Html, selector: &str) -> String {
    Selector::parse(selector)
        .ok()
        .and_then(|sel| doc.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_owned())
        .unwrap_or_default()
}

fn extract_value_near_name(html: &str, needle: &str) -> Option<String> {
    let i = html.find(needle)?;
    let after = &html[i..];
    let v_pos = after.find("value=\"")?;
    // Ensure value= is within the same tag (no > between)
    if after[..v_pos].contains('>') {
        return None;
    }
    let start = v_pos + 7;
    let end = after[start..].find('"')?;
    Some(after[start..start + end].to_owned())
}

fn hidden_value(html: &str, name: &str) -> String {
    let name_needle = format!("name=\"{name}\"");
    // Try name=...value=... order first, then value=...name=... order.
    extract_value_near_name(html, &name_needle)
        .or_else(|| {
            // Reverse: find value= that appears just before name=
            let mut pos = 0;
            while let Some(i) = html[pos..].find("value=\"") {
                let abs = pos + i;
                let tag_start = html[..abs].rfind('<').unwrap_or(0);
                let tag_slice = &html[tag_start..abs + 100.min(html.len() - abs)];
                if tag_slice.contains(&name_needle) {
                    let start = abs + 7;
                    if let Some(end) = html[start..].find('"') {
                        return Some(html[start..start + end].to_owned());
                    }
                }
                pos = abs + 1;
            }
            None
        })
        .unwrap_or_default()
}

/// Parses a `style="...width: N%"` attribute value into a rounded percentage.
fn percent_from_style(style: &str) -> Option<u32> {
    let i = style.find("width:")?;
    let after = style[i + 6..].trim_start();
    let end = after.find('%')?;
    after[..end]
        .trim()
        .parse::<f32>()
        .ok()
        .map(|f| f.round() as u32)
}

/// Extracts the progress percentage from the `edit-progress` progress bar's
/// inline `width` style. StoryGraph renders this bar for both page- and
/// time-tracked books, unlike the hidden percent input which only exists for
/// page-tracked books.
fn progress_percent_from_edit_progress(html: &str) -> Option<u32> {
    let i = html.find("edit-progress")?;
    percent_from_style(&html[i..])
}

fn parse_status_text(text: &str) -> Option<u8> {
    ReadingStatus::parse(text).map(|s| s as u8)
}

/// Parses a status from either the human-readable form ("Currently reading")
/// or the URL slug form ("currently-reading").
pub fn parse_status_arg(input: &str) -> Option<ReadingStatus> {
    ReadingStatus::parse(input).or_else(|| match input.trim().to_lowercase().as_str() {
        "to-read" => Some(ReadingStatus::ToRead),
        "currently-reading" => Some(ReadingStatus::CurrentlyReading),
        "did-not-finish" => Some(ReadingStatus::DidNotFinish),
        _ => None,
    })
}

fn status_to_str(status_id: u8) -> &'static str {
    ReadingStatus::from_id(status_id)
        .map(ReadingStatus::as_str)
        .unwrap_or("currently-reading")
}

/// Fetch /books/:slug and extract mutable form state needed for POST calls.
fn fetch_page_data(book_id: &str) -> Result<(String, PageData)> {
    let html = get(&format!("/books/{book_id}"))?;
    let csrf = csrf_token(&html).context("Could not find CSRF token on book page")?;
    let book_pages = hidden_value(&html, "read_status[book_num_of_pages]");

    Ok((html, PageData { csrf, book_pages }))
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns the logged-in user's slug. Uses `user_slug` from config if set,
/// otherwise scrapes the homepage nav for a `/users/` profile link.
pub fn get_user() -> Result<User> {
    require_credentials()?;

    if !CONFIG.user_slug.is_empty() {
        return Ok(User {
            username: CONFIG.user_slug.clone(),
            slug: CONFIG.user_slug.clone(),
        });
    }

    let html = get("/")?;
    let doc = Html::parse_document(&html);

    // The nav has a link to the user's profile: /users/:slug
    let sel = Selector::parse(r#"a[href^="/users/"]"#).expect("valid selector");
    let slug = doc
        .select(&sel)
        .filter_map(|el| el.value().attr("href"))
        .filter(|href| !href.contains("/sign_in") && !href.contains("/sign_up"))
        .filter_map(|href| href.strip_prefix("/users/"))
        .map(str::to_owned)
        .next()
        .context("Could not find user profile link — session may be expired")?;

    Ok(User {
        username: slug.clone(),
        slug,
    })
}

/// Searches StoryGraph by title/author and returns matching books.
/// Audio editions are filtered out.
pub fn search(query: &str) -> Result<Vec<Book>> {
    require_credentials()?;
    let html = get(&format!(
        "/browse?search_term={}",
        urlencoding::encode(query)
    ))?;
    let doc = Html::parse_document(&html);

    let container_sel = Selector::parse(".book-title-author-and-series").expect("valid selector");
    let title_sel = Selector::parse(r#"a[href^="/books/"]"#).expect("valid selector");
    let author_sel = Selector::parse(r#"a[href^="/authors/"]"#).expect("valid selector");

    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for container in doc.select(&container_sel) {
        let container_html = container.html();
        let container_doc = Html::parse_fragment(&container_html);

        let title_el = container_doc.select(&title_sel).next();
        if let Some(title_el) = title_el {
            let book_id = title_el
                .value()
                .attr("href")
                .and_then(|href| href.strip_prefix("/books/"))
                .map(str::to_owned);

            if let Some(book_id) = book_id {
                if seen.contains(&book_id) {
                    continue;
                }

                // Skip audio editions based on format info in nearby text
                let pane_text = container_html.to_lowercase();
                if pane_text.contains("format: audio") {
                    continue;
                }

                let title = title_el.text().collect::<String>().trim().to_owned();
                let author = container_doc
                    .select(&author_sel)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_owned())
                    .unwrap_or_else(|| "Unknown Author".to_owned());

                // Page count from "X pages" text nearby
                let pages = pane_text
                    .split_whitespace()
                    .zip(pane_text.split_whitespace().skip(1))
                    .find(|(_, next)| *next == "pages")
                    .and_then(|(n, _)| n.parse::<u32>().ok())
                    .unwrap_or(0);

                seen.insert(book_id.clone());
                results.push(Book {
                    id: book_id,
                    title,
                    author,
                    pages,
                });
            }
        }
    }

    debug_log!("search({query:?}) → {} results", results.len());
    Ok(results)
}

/// Returns what fraction of the meaningful words in `query_title` appear in
/// `result_title`.
pub fn title_match_score(query_title: &str, result_title: &str) -> f32 {
    const STOPS: &[&str] = &[
        "the", "a", "an", "of", "in", "by", "to", "and", "or", "for", "at", "its", "is",
    ];

    let tokenize = |s: &str| -> Vec<String> {
        s.to_lowercase()
            // Split on non-alphanumeric so volume/edition numbers ("Vol. 1" vs "Vol. 2")
            // survive as their own tokens instead of vanishing as delimiter characters.
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| {
                !w.is_empty()
                    && !STOPS.contains(w)
                    && (w.len() > 1 || w.chars().all(|c| c.is_ascii_digit()))
            })
            .map(str::to_owned)
            .collect()
    };

    let query_words = tokenize(query_title);
    if query_words.is_empty() {
        return 0.0;
    }

    let result_words: std::collections::HashSet<String> =
        tokenize(result_title).into_iter().collect();
    let matches = query_words
        .iter()
        .filter(|w| result_words.contains(*w))
        .count();
    matches as f32 / query_words.len() as f32
}

/// Fetches the current user's reading status and progress for a book.
pub fn get_user_book(book_id: &str) -> Result<UserBook> {
    require_credentials()?;
    let html = get(&format!("/books/{book_id}"))?;
    parse_user_book(book_id, &html)
}

fn parse_user_book(book_id: &str, html: &str) -> Result<UserBook> {
    let doc = Html::parse_document(html);

    let status_text = text_of(&doc, ".read-status-label");
    let status_id = parse_status_text(&status_text);

    // Try progress bar width first (most reliable), then hidden inputs.
    let progress_percent = progress_percent_from_edit_progress(html).unwrap_or_else(|| {
        hidden_value(html, "read_status[last_reached_percent]")
            .parse()
            .unwrap_or(0)
    });

    let progress_pages: u32 = hidden_value(html, "read_status[last_reached_pages]")
        .parse()
        .unwrap_or_else(|_| {
            extract_value_near_name(html, "read-status-last-reached-pages")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        });
    let book_pages: u32 = hidden_value(html, "read_status[book_num_of_pages]")
        .parse()
        .unwrap_or_else(|_| {
            extract_value_near_name(html, "read-status-book-num-of-pages")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        });

    let progress_type = Selector::parse(".read-status-progress-type option[selected]")
        .ok()
        .and_then(|sel| doc.select(&sel).next())
        .and_then(|el| el.value().attr("value"))
        .unwrap_or("percentage")
        .to_owned();

    let edition_format = {
        let sel = Selector::parse(".edition-info p").expect("valid selector");
        doc.select(&sel)
            .find(|el| el.text().collect::<String>().contains("Format:"))
            .map(|el| {
                el.text()
                    .collect::<String>()
                    .replacen("Format:", "", 1)
                    .trim()
                    .to_owned()
            })
            .unwrap_or_default()
    };

    Ok(UserBook {
        book_id: book_id.to_owned(),
        status_id,
        progress_percent,
        progress_pages,
        book_pages,
        progress_type,
        edition_format,
    })
}

/// Sets the reading status of a book (to-read, reading, read, paused, dnf).
/// POSTs to `/update-status.js` and re-fetches the book page to confirm.
pub fn update_status(book_id: &str, status_id: u8) -> Result<UserBook> {
    require_credentials()?;
    let (_html, data) = fetch_page_data(book_id)?;
    let status_str = status_to_str(status_id);

    let path = format!("/update-status.js?book_id={book_id}&status={status_str}");
    post_form(&path, &[("authenticity_token", data.csrf.as_str())])?;

    // Re-fetch to return current state.
    parse_user_book(book_id, &get(&format!("/books/{book_id}"))?)
}

/// Updates reading progress for a book. `progress_type` is `"percentage"` or `"pages"`.
/// Fetches the book page first to extract the CSRF token and current page count.
/// If the book has no reading status, automatically sets it to "Currently Reading" first.
pub fn update_progress(book_id: &str, progress_type: &str, value: u32) -> Result<UserBook> {
    require_credentials()?;
    let (html, mut data) = fetch_page_data(book_id)?;

    let current_status = parse_user_book(book_id, &html)?.status_id;
    if current_status.is_none() {
        update_status(book_id, ReadingStatus::CurrentlyReading as u8)?;
        let (_, fresh) = fetch_page_data(book_id)?;
        data = fresh;
    } else if current_status == Some(ReadingStatus::Read as u8) {
        update_status(book_id, ReadingStatus::Rereading as u8)?;
        let (_, fresh) = fetch_page_data(book_id)?;
        data = fresh;
    }

    let value_str = value.to_string();

    post_form(
        "/update-progress",
        &[
            ("authenticity_token", &data.csrf),
            ("read_status[progress_number]", &value_str),
            ("read_status[progress_type]", progress_type),
            ("read_status[book_num_of_pages]", &data.book_pages),
            ("book_id", book_id),
            ("on_book_page", "true"),
        ],
    )?;

    parse_user_book(book_id, &get(&format!("/books/{book_id}"))?)
}

/// Logs in with email + password and returns the session cookies StoryGraph
/// issued. Uses a fresh cookie jar (not the config-seeded singleton), mirrors
/// the browser's sign-in form POST, and reads the cookies back from the jar.
///
/// NOTE: as of late 2026 `/users/sign_in` sits behind a Cloudflare bot
/// challenge (HTTP 403 "Just a moment...") for non-browser clients, so this
/// currently fails with "Could not find CSRF token" unless the challenge is
/// lifted or a valid `cf_clearance` cookie is already present in the jar
/// (e.g. pre-seeded from a browser session).
pub fn login(email: &str, password: &str) -> Result<LoginResult> {
    let base: Url = BASE_URL.parse().expect("BASE_URL is valid");
    let jar = Arc::new(Jar::default());
    let client = build_client(jar.clone()).context("Failed to build login client")?;

    let sign_in_url = format!("{BASE_URL}/users/sign_in");

    // 1. Load the sign-in page to obtain a CSRF token.
    let res = client
        .get(&sign_in_url)
        .send()
        .context("Failed to load sign-in page")?;
    let html = res.text().context("Failed to read sign-in page body")?;
    let csrf = csrf_token(&html).context("Could not find CSRF token on sign-in page")?;

    // 2. Submit credentials, mirroring the browser form.
    let res = client
        .post(&sign_in_url)
        .header("Origin", BASE_URL)
        .header("Referer", &sign_in_url)
        .form(&[
            ("authenticity_token", csrf.as_str()),
            ("user[email]", email),
            ("user[password]", password),
            ("commit", "Sign in"),
        ])
        .send()
        .context("Failed to submit sign-in form")?;

    // 3. On success StoryGraph redirects away from the sign-in page.
    if res.url().path().contains("/users/sign_in") {
        bail!("Login failed — check your email address and password.");
    }

    // 4. Read the session cookies back from the jar (a "name=value; name=value"
    //    header string).
    let cookies_header = jar.cookies(&base).context("Login produced no cookies")?;
    let cookies_str = cookies_header.to_str().unwrap_or("");
    let mut session_cookie = String::new();
    let mut remember_user_token = String::new();
    for pair in cookies_str.split(';') {
        if let Some((name, value)) = pair.trim().split_once('=') {
            match name.trim() {
                "_storygraph_session" => session_cookie = value.trim().to_owned(),
                "remember_user_token" => remember_user_token = value.trim().to_owned(),
                _ => {}
            }
        }
    }

    if session_cookie.is_empty() {
        bail!("Login did not return a session cookie.");
    }

    Ok(LoginResult {
        session_cookie,
        remember_user_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_match_score_identical_is_one() {
        assert!(
            (title_match_score("The Great Gatsby", "The Great Gatsby") - 1.0).abs() < f32::EPSILON
        );
    }

    #[test]
    fn title_match_score_empty_query_is_zero() {
        assert_eq!(title_match_score("", "anything"), 0.0);
    }

    #[test]
    fn title_match_score_volume_differs() {
        // "Vol. 1" vs "Vol. 2" must not score as identical (3/4 words match).
        let s = title_match_score("Harry Potter Vol. 1", "Harry Potter Vol. 2");
        assert!((s - 0.75).abs() < f32::EPSILON, "got {s}");
    }

    #[test]
    fn parse_status_text_cases() {
        assert_eq!(parse_status_text("Currently reading"), Some(2));
        assert_eq!(parse_status_text("Rereading"), Some(6));
        assert_eq!(parse_status_text("Want to Read"), Some(1));
        assert_eq!(parse_status_text("To-read"), Some(1));
        assert_eq!(parse_status_text("Paused"), Some(4));
        assert_eq!(parse_status_text("Did not finish"), Some(5));
        assert_eq!(parse_status_text("Read"), Some(3));
        assert_eq!(parse_status_text("banana"), None);
    }

    #[test]
    fn status_to_str_roundtrip() {
        assert_eq!(status_to_str(1), "to-read");
        assert_eq!(status_to_str(2), "currently-reading");
        assert_eq!(status_to_str(3), "read");
        assert_eq!(status_to_str(4), "paused");
        assert_eq!(status_to_str(5), "did-not-finish");
        assert_eq!(status_to_str(6), "rereading");
        assert_eq!(status_to_str(99), "currently-reading"); // unknown falls back
    }

    #[test]
    fn csrf_extracted_from_meta_and_input() {
        assert_eq!(
            csrf_token(r#"<meta name="csrf-token" content="TOKEN123">"#),
            Some("TOKEN123".to_owned())
        );
        assert_eq!(
            csrf_token(r#"<input name="authenticity_token" value="TOKEN456">"#),
            Some("TOKEN456".to_owned())
        );
    }
}
