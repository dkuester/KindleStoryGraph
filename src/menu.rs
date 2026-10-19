//! KUAL menu generation and StoryGraph match discovery.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::{self, CONFIG};
use crate::reader::Reader;
use crate::state::State;
use crate::storygraph;

/// One StoryGraph search hit, cached for the KUAL pick-list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub author: String,
}

/// Cached search results keyed by Kindle `cde_key`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchCache {
    pub books: HashMap<String, Vec<Candidate>>,
}

impl SearchCache {
    pub fn load(path: &Path) -> Result<SearchCache> {
        if !path.exists() {
            return Ok(SearchCache::default());
        }
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if content.trim().is_empty() {
            return Ok(SearchCache::default());
        }
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir of {}", path.display()))?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))
    }
}

/// `cde_key → storygraph_id` for every book that already resolves to a
/// StoryGraph book. Manual mappings take precedence; persisted sync state
/// fills in the rest.
fn linked_keys(manual_mappings_path: &Path, state_path: &Path) -> HashMap<String, String> {
    let mut linked = config::load_manual_mappings(manual_mappings_path);
    if let Ok(state) = State::load(state_path) {
        for (key, book) in state.books {
            linked.entry(key).or_insert(book.storygraph_id);
        }
    }
    linked
}

/// Build the KUAL `menu.json`: the allowlist selection (with `[sync]`/`[linked]`
/// markers), a "Link books" picker rendered from the cached match candidates,
/// and the standard sync/daemon actions. A missing or unreadable `cc.db` or
/// cache only empties the affected submenus (the menu stays usable).
pub fn build_menu(cc_db: &Path) -> Value {
    render(cc_db).0
}

/// Render the menu. The second value is `false` when `cc.db` yielded no books
/// (unreadable, locked, or genuinely empty) — i.e. when the book list must not
/// be trusted as a replacement for what is already on disk.
///
/// KUAL Next notes (verified against its source: `src/menu.c` + `src/ui_fbink.c`):
/// - `exitmenu` defaults to **true**: the launcher replaces itself with the
///   action and the menu is gone. Only `"exitmenu": false` keeps it open.
/// - `refresh` is honoured **only** for a non-exiting entry, and the launcher
///   re-reads `menu.json` ~250 ms after starting the action — so every action
///   that pairs `refresh` with `exitmenu: false` must be able to rewrite
///   `menu.json` within that window (hence one binary call per action).
/// - `checked` means "draw a check after the button is pressed" (KUAL's
///   transient indicator); it is NOT read as an initial state. Persisted state
///   therefore has to live in `name`.
fn render(cc_db: &Path) -> (Value, bool) {
    let allowlist = config::load_allowlist(Path::new(&CONFIG.allowlist_path));
    let linked = linked_keys(
        Path::new(&CONFIG.manual_mappings_path),
        Path::new(&CONFIG.state_path),
    );
    let cache = SearchCache::load(Path::new(&CONFIG.search_cache_path)).unwrap_or_default();
    render_with(cc_db, &allowlist, &linked, &cache)
}

/// `render` with its inputs injected (testable without touching `CONFIG`).
pub fn render_with(
    cc_db: &Path,
    allowlist: &HashSet<String>,
    linked: &HashMap<String, String>,
    cache: &SearchCache,
) -> (Value, bool) {
    let snapshot = Reader::open_read_only(cc_db.to_str().unwrap_or_default())
        .ok()
        .and_then(|reader| {
            reader
                .snapshot(CONFIG.finish_threshold, CONFIG.min_progress)
                .ok()
        })
        .unwrap_or_default();
    let has_books = !snapshot.is_empty();

    let mut selection: Vec<Value> = Vec::new();
    for book in &snapshot {
        let mut name = book.title.clone();
        if allowlist.contains(&book.cde_key) {
            name.push_str("  [sync]");
        }
        if linked.contains_key(&book.cde_key) {
            name.push_str("  [linked]");
        }
        selection.push(json!({
            "name": name,
            "action": "bin/toggle-book.sh",
            "params": book.cde_key,
            "exitmenu": false,
            "refresh": true,
        }));
    }
    if selection.is_empty() {
        selection.push(json!({
            "name": "(no books with progress yet)",
            "action": "bin/refresh-menu.sh",
            "exitmenu": false,
            "refresh": true,
        }));
    }

    let mut link_items: Vec<Value> = Vec::new();
    for book in &snapshot {
        if linked.contains_key(&book.cde_key) {
            continue;
        }
        let Some(candidates) = cache.books.get(&book.cde_key) else {
            continue;
        };
        if candidates.is_empty() {
            continue;
        }
        let mut entries: Vec<Value> = Vec::new();
        for c in candidates {
            let label = if c.author.is_empty() || c.author == "Unknown Author" {
                c.title.clone()
            } else {
                format!("{} — {}", c.title, c.author)
            };
            entries.push(json!({
                "name": label,
                "action": "bin/link-book.sh",
                "params": format!("{} {}", book.cde_key, c.id),
                "exitmenu": false,
                "refresh": true,
            }));
        }
        link_items.push(json!({
            "name": book.title,
            "items": entries,
        }));
    }

    let mut items: Vec<Value> = vec![json!({
        "name": "Sync selection",
        "priority": 1,
        "items": selection,
    })];
    if !link_items.is_empty() {
        items.push(json!({
            "name": "Link books",
            "priority": 2,
            "items": link_items,
        }));
    }
    items.push(json!({
        "name": "Find matches for books",
        "priority": 3,
        "action": "bin/find-matches.sh",
    }));
    items.push(json!({
        "name": "Refresh book list",
        "priority": 4,
        "action": "bin/refresh-menu.sh",
        "exitmenu": false,
        "refresh": true,
    }));
    items.push(json!({
        "name": "Sync now",
        "priority": 5,
        "action": "bin/sync-once.sh",
    }));
    items.push(json!({
        "name": "Start sync daemon",
        "priority": 6,
        "action": "bin/start.sh",
    }));
    items.push(json!({
        "name": "Stop sync daemon",
        "priority": 7,
        "action": "bin/stop.sh",
    }));

    (json!({ "items": items }), has_books)
}

/// Rendered menu → `CONFIG.menu_path`, via a temp file + rename so the
/// launcher never reads a half-written file. Refuses to replace a good menu
/// with an empty book list (a locked/unreadable `cc.db` must not wipe the
/// selection the user sees).
pub fn write_menu(cc_db: &Path) -> Result<()> {
    let (menu, has_books) = render(cc_db);
    if !has_books {
        bail!(
            "cc.db returned no books — leaving {} untouched",
            CONFIG.menu_path
        );
    }
    let path = Path::new(&CONFIG.menu_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir of {}", path.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", CONFIG.menu_path));
    std::fs::write(&tmp, serde_json::to_string_pretty(&menu)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

/// Search StoryGraph for every unlinked book with progress and cache the top
/// `max_per_book` candidates (query = title + author). Skips — and logs — books
/// whose search fails rather than aborting the batch. Returns the number of
/// books searched; errors out only if every search failed.
pub fn find_matches(cc_db: &Path, max_per_book: usize) -> Result<usize> {
    let linked = linked_keys(
        Path::new(&CONFIG.manual_mappings_path),
        Path::new(&CONFIG.state_path),
    );
    let reader = Reader::open_read_only(cc_db.to_str().unwrap_or_default())?;
    let snapshot = reader.snapshot(CONFIG.finish_threshold, CONFIG.min_progress)?;

    let mut cache = SearchCache::load(Path::new(&CONFIG.search_cache_path)).unwrap_or_default();
    let mut searched = 0usize;
    let mut failed = 0usize;

    for book in &snapshot {
        if linked.contains_key(&book.cde_key) {
            continue;
        }
        let query = if book.author.is_empty() {
            book.title.clone()
        } else {
            format!("{} {}", book.title, book.author)
        };
        match storygraph::search(&query) {
            Ok(results) => {
                let candidates: Vec<Candidate> = results
                    .into_iter()
                    .take(max_per_book)
                    .map(|b| Candidate {
                        id: b.id,
                        title: b.title,
                        author: b.author,
                    })
                    .collect();
                cache.books.insert(book.cde_key.clone(), candidates);
                searched += 1;
            }
            Err(e) => {
                failed += 1;
                eprintln!("search failed for '{}': {e:#}", book.title);
            }
        }
    }

    cache.save(Path::new(&CONFIG.search_cache_path))?;

    // The candidate list only shows up in the menu once menu.json is
    // regenerated — do it here so "Find matches" needs just one binary call.
    if let Err(e) = write_menu(cc_db) {
        eprintln!("warning: could not update the KUAL menu: {e:#}");
    }

    if searched == 0 && failed > 0 {
        bail!("all {failed} searches failed — check session_cookie in config.ini and the network");
    }
    Ok(searched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::BookState;

    #[test]
    fn search_cache_roundtrip() {
        let path = std::env::temp_dir().join("ksg_test_search_cache.json");
        let mut cache = SearchCache::default();
        cache.books.insert(
            "KEY1".to_owned(),
            vec![Candidate {
                id: "id-1".to_owned(),
                title: "Title".to_owned(),
                author: "Author".to_owned(),
            }],
        );
        cache.save(&path).unwrap();
        let loaded = SearchCache::load(&path).unwrap();
        assert_eq!(loaded.books["KEY1"][0].id, "id-1");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn search_cache_missing_is_empty() {
        let cache =
            SearchCache::load(&std::env::temp_dir().join("ksg_nonexistent_cache.json")).unwrap();
        assert!(cache.books.is_empty());
    }

    #[test]
    fn linked_keys_manual_wins_over_state() {
        let dir = std::env::temp_dir();
        let manual = dir.join("ksg_test_linked_manual.ini");
        let state = dir.join("ksg_test_linked_state.json");
        std::fs::write(&manual, "KEY1=manual-id\n").unwrap();

        let mut st = State::default();
        st.books.insert(
            "KEY1".to_owned(),
            BookState {
                storygraph_id: "state-id".to_owned(),
                status: "currently-reading".to_owned(),
                last_percent: 10.0,
                last_synced_at: 0,
            },
        );
        st.books.insert(
            "KEY2".to_owned(),
            BookState {
                storygraph_id: "state-id-2".to_owned(),
                status: "currently-reading".to_owned(),
                last_percent: 10.0,
                last_synced_at: 0,
            },
        );
        st.save(&state).unwrap();

        let linked = linked_keys(&manual, &state);
        assert_eq!(linked["KEY1"], "manual-id"); // manual wins over state
        assert_eq!(linked["KEY2"], "state-id-2"); // state fills in the rest

        std::fs::remove_file(&manual).ok();
        std::fs::remove_file(&state).ok();
    }

    /// Temp cc.db with `rows` as (cde_key, title, percent) and one freshly
    /// read book each.
    fn temp_cc_db(name: &str, rows: &[(&str, &str, f64)]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::remove_file(&path).ok();
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE Entries (
                p_uuid TEXT PRIMARY KEY,
                p_type TEXT,
                p_isArchived INTEGER,
                p_cdeKey TEXT,
                p_cdeType TEXT,
                p_titles_0_nominal TEXT,
                j_credits TEXT,
                p_percentFinished REAL,
                p_lastAccess INTEGER
            );",
        )
        .unwrap();
        for (i, (key, title, percent)) in rows.iter().enumerate() {
            conn.execute(
                "INSERT INTO Entries VALUES (?1, 'Entry:Item', 0, ?2, 'EBOK', ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    format!("uuid-{i}"),
                    key,
                    title,
                    r#"[{"name":{"display":"An Author"},"kind":"Author"}]"#,
                    percent,
                    100i64 - i as i64
                ],
            )
            .unwrap();
        }
        path
    }

    fn all_entries(menu: &Value) -> Vec<Value> {
        fn walk(items: &Value, out: &mut Vec<Value>) {
            for item in items.as_array().into_iter().flatten() {
                if let Some(children) = item.get("items") {
                    walk(children, out);
                } else {
                    out.push(item.clone());
                }
            }
        }
        let mut out = Vec::new();
        walk(menu.get("items").unwrap(), &mut out);
        out
    }

    #[test]
    fn toggle_entries_keep_the_menu_open_and_refresh() {
        let db = temp_cc_db("ksg_test_menu_cc.db", &[("KEY1", "Dune", 42.0)]);
        let mut allowlist = HashSet::new();
        allowlist.insert("KEY1".to_owned());

        let (menu, has_books) =
            render_with(&db, &allowlist, &HashMap::new(), &SearchCache::default());
        assert!(has_books);
        let toggle = all_entries(&menu)
            .into_iter()
            .find(|e| e["action"] == "bin/toggle-book.sh")
            .expect("toggle entry");
        // KUAL Next defaults exitmenu to true (menu closes) and ignores
        // refresh for exiting entries — both flags are load-bearing.
        assert_eq!(toggle["exitmenu"], false);
        assert_eq!(toggle["refresh"], true);
        assert_eq!(toggle["params"], "KEY1");
        // Persisted state lives in the name: KUAL's `checked` is press-only.
        assert_eq!(toggle["name"], "Dune  [sync]");
        assert!(toggle.get("checked").is_none());

        std::fs::remove_file(&db).ok();
    }

    #[test]
    fn selection_marks_linked_books_and_orders_markers() {
        let db = temp_cc_db(
            "ksg_test_menu_cc2.db",
            &[("KEY1", "Dune", 42.0), ("KEY2", "Emma", 88.0)],
        );
        let mut allowlist = HashSet::new();
        allowlist.insert("KEY1".to_owned());
        let mut linked = HashMap::new();
        linked.insert("KEY1".to_owned(), "sg-1".to_owned());

        let (menu, _) = render_with(&db, &allowlist, &linked, &SearchCache::default());
        let names: Vec<String> = all_entries(&menu)
            .into_iter()
            .filter(|e| e["action"] == "bin/toggle-book.sh")
            .map(|e| e["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(names.contains(&"Dune  [sync]  [linked]".to_owned()));
        assert!(names.contains(&"Emma".to_owned()));

        std::fs::remove_file(&db).ok();
    }

    #[test]
    fn link_picker_uses_cached_candidates_for_unlinked_books() {
        let db = temp_cc_db("ksg_test_menu_cc3.db", &[("KEY1", "Dune", 42.0)]);
        let mut cache = SearchCache::default();
        cache.books.insert(
            "KEY1".to_owned(),
            vec![Candidate {
                id: "sg-42".to_owned(),
                title: "Dune".to_owned(),
                author: "Frank Herbert".to_owned(),
            }],
        );

        let (menu, _) = render_with(&db, &HashSet::new(), &HashMap::new(), &cache);
        let link = all_entries(&menu)
            .into_iter()
            .find(|e| e["action"] == "bin/link-book.sh")
            .expect("link entry");
        assert_eq!(link["params"], "KEY1 sg-42");
        assert_eq!(link["exitmenu"], false);
        assert_eq!(link["refresh"], true);
        assert_eq!(link["name"], "Dune — Frank Herbert");

        std::fs::remove_file(&db).ok();
    }

    #[test]
    fn unreadable_cc_db_reports_no_books() {
        let missing = std::env::temp_dir().join("ksg_test_menu_missing.db");
        std::fs::remove_file(&missing).ok();
        let (menu, has_books) = render_with(
            &missing,
            &HashSet::new(),
            &HashMap::new(),
            &SearchCache::default(),
        );
        assert!(
            !has_books,
            "a missing cc.db must not count as an empty library"
        );
        // The menu itself still renders, with the placeholder entry.
        assert!(
            all_entries(&menu)
                .iter()
                .any(|e| e["action"] == "bin/refresh-menu.sh")
        );
    }
}
