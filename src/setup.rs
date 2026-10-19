//! Host-side setup helpers: cookie import and the `doctor` diagnostic.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{self, CONFIG};
use crate::error::UserError;
use crate::reader::Reader;
use crate::state::{self, State};

/// Parses a Netscape-format `cookies.txt` body into `name → value` (the last
/// occurrence of a name wins). Lines are `#`-comment, blank, or
/// `domain  flag  path  secure  expiry  name  value` (tab- or space-separated).
pub fn parse_cookies(content: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else {
            line.split_whitespace().collect()
        };
        if fields.len() >= 7 {
            out.insert(fields[5].to_owned(), fields[6].to_owned());
        }
    }
    out
}

/// Extracts `_storygraph_session` and `remember_user_token` from an exported
/// cookie file and writes them into `config.ini` (next to the binary, or the
/// path from `KINDLE_STORYGRAPH_CONFIG`), preserving every other key.
pub fn import_cookies(path: &Path) -> Result<()> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let cookies = parse_cookies(&content);
    let session = cookies.get("_storygraph_session");
    let token = cookies.get("remember_user_token");
    let (Some(session), Some(token)) = (session, token) else {
        bail!(UserError(format!(
            "{} contains neither `_storygraph_session` nor `remember_user_token` — \
             export cookies for app.thestorygraph.com in Netscape format",
            path.display()
        )));
    };
    let cfg = config::config_path()?;
    config::set_ini_keys(
        &cfg,
        &[
            ("session_cookie", session.as_str()),
            ("remember_user_token", token.as_str()),
        ],
    )?;
    println!("Imported StoryGraph session into {}", cfg.display());
    Ok(())
}

fn set_str(v: &str) -> String {
    if v.is_empty() {
        "not set".to_owned()
    } else {
        "set".to_owned()
    }
}

/// Prints a human-readable health report (config, cc.db, allowlist, state,
/// optional network check). Report-only: always returns `Ok` (exit code 0).
pub fn doctor(cc_db: &Path, network: bool) -> Result<()> {
    println!("KindleStoryGraph doctor");
    println!();

    let cfg_path = config::config_path()?;
    println!(
        "config.ini       {}",
        if cfg_path.exists() {
            format!("found ({})", cfg_path.display())
        } else {
            "missing — copy config_example.ini to config.ini".to_owned()
        }
    );
    println!("  session cookie : {}", set_str(&CONFIG.session_cookie));
    println!(
        "  remember token : {}",
        set_str(&CONFIG.remember_user_token)
    );
    println!("  sync_scope     : {}", CONFIG.sync_scope);

    println!();
    match Reader::open_read_only(cc_db.to_str().unwrap_or_default())
        .and_then(|r| r.snapshot(CONFIG.finish_threshold, CONFIG.min_progress))
    {
        Ok(snap) => println!(
            "cc.db            OK — {} book(s) with progress >= {:.1}%",
            snap.len(),
            CONFIG.min_progress
        ),
        Err(e) => println!("cc.db            UNREADABLE — {e:#}"),
    }

    let allowlist = config::load_allowlist(Path::new(&CONFIG.allowlist_path));
    println!("allowlist        {} book(s) selected", allowlist.len());

    let state = State::load(Path::new(&CONFIG.state_path)).unwrap_or_default();
    let last = state.books.values().map(|b| b.last_synced_at).max();
    println!(
        "state            {} book(s) synced, {}",
        state.books.len(),
        last.map(state::humanize_ago)
            .unwrap_or_else(|| "never synced".to_owned())
    );

    if network {
        println!();
        match crate::storygraph::get_user() {
            Ok(u) => println!("network          logged in as {} ({})", u.username, u.slug),
            Err(e) => println!("network          FAILED — session likely expired ({e:#})"),
        }
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cookies_netscape_format() {
        let content = "\
# Netscape HTTP Cookie File
.app.thestorygraph.com\tTRUE\t/\tTRUE\t1799999999\t_storygraph_session\tSESSIONABC
app.thestorygraph.com\tTRUE\t/\tTRUE\t1799999999\tremember_user_token\tTOKENXYZ
";
        let c = parse_cookies(content);
        assert_eq!(c["_storygraph_session"], "SESSIONABC");
        assert_eq!(c["remember_user_token"], "TOKENXYZ");
    }

    #[test]
    fn parse_cookies_keeps_values_with_spaces_when_tab_separated() {
        let c = parse_cookies(".app\tTRUE\t/\tTRUE\t0\tfoo\tbar baz\n");
        assert_eq!(c.get("foo").map(String::as_str), Some("bar baz"));
    }

    #[test]
    fn parse_cookies_last_occurrence_wins() {
        let content = "a\tTRUE\t/\tTRUE\t1\tk\told\na\tTRUE\t/\tTRUE\t2\tk\tnew\n";
        assert_eq!(parse_cookies(content)["k"], "new");
    }

    #[test]
    fn parse_cookies_skips_comments_and_short_lines() {
        let c = parse_cookies("# comment\n\nnot enough fields\n");
        assert!(c.is_empty());
    }
}
