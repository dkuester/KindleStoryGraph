use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};

use crate::fsutil::write_atomic;

#[derive(Debug, Clone)]
pub struct Config {
    pub session_cookie: String,
    pub remember_user_token: String,
    pub user_slug: String,
    pub language: String,
    pub debug: bool,
    pub cc_db_path: String,
    pub state_path: String,
    pub finish_threshold: f64,
    pub progress_step: f64,
    pub poll_interval: u64,
    pub match_threshold: f32,
    pub min_progress: f64,
    pub manual_mappings_path: String,
    pub sync_scope: String,
    pub allowlist_path: String,
    pub search_cache_path: String,
    pub menu_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            session_cookie: String::new(),
            remember_user_token: String::new(),
            user_slug: String::new(),
            language: "en".to_owned(),
            debug: false,
            cc_db_path: "/var/local/cc.db".to_owned(),
            state_path: "/mnt/us/extensions/kindlestorygraph/state.json".to_owned(),
            finish_threshold: 99.5,
            progress_step: 1.0,
            poll_interval: 900,
            match_threshold: 0.6,
            min_progress: 2.0,
            manual_mappings_path: "/mnt/us/extensions/kindlestorygraph/manual_mappings.ini"
                .to_owned(),
            sync_scope: "allowlist".to_owned(),
            allowlist_path: "/mnt/us/extensions/kindlestorygraph/allowlist.ini".to_owned(),
            search_cache_path: "/mnt/us/extensions/kindlestorygraph/search_cache.json".to_owned(),
            menu_path: "/mnt/us/extensions/kindlestorygraph/menu.json".to_owned(),
        }
    }
}

impl Config {
    /// Whether the sync covers every book (`sync_scope=all`) rather than only
    /// the allowlisted books.
    pub fn sync_all(&self) -> bool {
        self.sync_scope.eq_ignore_ascii_case("all")
    }
}

/// Parses bare `key=value` INI content (the format `config.ini` uses).
/// Blank lines, `#`/`;` comments, and a `[General]` section header are
/// ignored; unknown keys are ignored.
pub fn parse_ini(content: &str) -> Config {
    let mut cfg = Config::default();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line == "[General]"
        {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "session_cookie" => cfg.session_cookie = value.to_owned(),
            "remember_user_token" => cfg.remember_user_token = value.to_owned(),
            "user_slug" => cfg.user_slug = value.to_owned(),
            "language" => cfg.language = value.to_owned(),
            "debug" => cfg.debug = value.eq_ignore_ascii_case("true") || value == "1",
            "cc_db_path" => cfg.cc_db_path = value.to_owned(),
            "state_path" => cfg.state_path = value.to_owned(),
            "finish_threshold" => {
                cfg.finish_threshold = value.parse().unwrap_or(cfg.finish_threshold)
            }
            "progress_step" => cfg.progress_step = value.parse().unwrap_or(cfg.progress_step),
            "poll_interval" => cfg.poll_interval = value.parse().unwrap_or(cfg.poll_interval),
            "match_threshold" => cfg.match_threshold = value.parse().unwrap_or(cfg.match_threshold),
            "min_progress" => cfg.min_progress = value.parse().unwrap_or(cfg.min_progress),
            "manual_mappings_path" => cfg.manual_mappings_path = value.to_owned(),
            "sync_scope" => cfg.sync_scope = value.to_owned(),
            "allowlist_path" => cfg.allowlist_path = value.to_owned(),
            "search_cache_path" => cfg.search_cache_path = value.to_owned(),
            "menu_path" => cfg.menu_path = value.to_owned(),
            _ => {}
        }
    }
    cfg
}

/// Parses `cde_key = storygraph_id` lines into a map. Ignores comments,
/// blank lines, and malformed lines.
pub fn parse_manual_mappings(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    map
}

/// Loads `manual_mappings.ini`; a missing file yields an empty map.
pub fn load_manual_mappings(path: &Path) -> HashMap<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_manual_mappings(&content),
        Err(_) => HashMap::new(),
    }
}

/// Parses a bare `cde_key` allowlist (one key per line; an optional `=…`
/// suffix is tolerated, so `key=1` also works). Comments and blank lines are
/// skipped.
pub fn parse_allowlist(content: &str) -> HashSet<String> {
    content
        .lines()
        .filter_map(|line| {
            let key = line.split('=').next()?.trim();
            if key.is_empty() || key.starts_with('#') || key.starts_with(';') {
                None
            } else {
                Some(key.to_owned())
            }
        })
        .collect()
}

/// Loads `allowlist.ini`; a missing file yields an empty set.
pub fn load_allowlist(path: &Path) -> HashSet<String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_allowlist(&content),
        Err(_) => HashSet::new(),
    }
}

/// Toggles `key` in the allowlist file, preserving other lines (including
/// comments). Returns the new state: `true` if the book is now allowed.
pub fn toggle_allowlist(path: &Path, key: &str) -> Result<bool> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let key = key.trim();
    let mut present = false;
    let mut kept: Vec<&str> = Vec::new();
    for line in content.lines() {
        let bare = line.split('=').next().unwrap_or(line).trim();
        if bare == key {
            present = true;
        } else {
            kept.push(line);
        }
    }
    let mut out = kept.join("\n");
    if !present {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(key);
    }
    out.push('\n');
    write_atomic(path, out.as_bytes())
        .with_context(|| format!("writing allowlist at {}", path.display()))?;
    Ok(!present)
}

/// Removes `key` from the allowlist file, preserving other lines (including
/// comments). Returns whether it was present; the file is only rewritten
/// (atomically) when something changed.
pub fn remove_from_allowlist(path: &Path, key: &str) -> Result<bool> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let key = key.trim();
    let mut present = false;
    let mut kept: Vec<&str> = Vec::new();
    for line in content.lines() {
        let bare = line.split('=').next().unwrap_or(line).trim();
        if bare == key {
            present = true;
        } else {
            kept.push(line);
        }
    }
    if !present {
        return Ok(false);
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    write_atomic(path, out.as_bytes())
        .with_context(|| format!("writing allowlist at {}", path.display()))?;
    Ok(true)
}

/// Sets `cde_key = storygraph_id` in the manual mappings file, replacing any
/// existing entry for that key while preserving other lines (including
/// comments).
pub fn add_manual_mapping(path: &Path, cde_key: &str, storygraph_id: &str) -> Result<()> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let key = cde_key.trim();
    let mut kept: Vec<&str> = Vec::new();
    for line in content.lines() {
        let bare = line.split('=').next().unwrap_or(line).trim();
        if bare != key {
            kept.push(line);
        }
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(key);
    out.push('=');
    out.push_str(storygraph_id.trim());
    out.push('\n');
    write_atomic(path, out.as_bytes())
        .with_context(|| format!("writing manual mappings at {}", path.display()))?;
    Ok(())
}

/// Writes `updates` into a bare `key=value` INI file: an existing key's value
/// is replaced in place, a missing key is appended at the end. Comments, blank
/// lines and unrelated keys are preserved. Written atomically (see
/// [`write_atomic`]) so a suspend mid-write can't leave a truncated config.
pub fn set_ini_keys(path: &Path, updates: &[(&str, &str)]) -> Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    let mut used = vec![false; updates.len()];

    for line in existing.lines() {
        let trimmed = line.trim();
        let mut handled = false;
        if !(trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with(';')
            || trimmed == "[General]")
            && let Some((key, _)) = line.split_once('=')
        {
            let key = key.trim();
            for (i, (k, v)) in updates.iter().enumerate() {
                if *k == key {
                    out.push(format!("{key}={v}"));
                    used[i] = true;
                    handled = true;
                    break;
                }
            }
        }
        if !handled {
            out.push(line.to_owned());
        }
    }

    for (i, (k, v)) in updates.iter().enumerate() {
        if !used[i] {
            out.push(format!("{k}={v}"));
        }
    }

    let content = format!("{}\n", out.join("\n"));
    write_atomic(path, content.as_bytes())
}

/// Resolves the config path: the `KINDLE_STORYGRAPH_CONFIG` env var if set
/// (handy for host testing), otherwise `config.ini` next to the binary.
pub fn config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("KINDLE_STORYGRAPH_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe().context("Failed to get current binary path")?;
    Ok(exe
        .parent()
        .context("Failed to get current binary directory")?
        .join("config.ini"))
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(|| {
    let load = || -> Result<Config> {
        let path = config_path()?;
        if !path.exists() {
            let example = path.with_file_name("config_example.ini");
            if example.exists() {
                fs::copy(&example, &path)
                    .context("Failed to copy config_example.ini to config.ini")?;
            }
        }
        let content = fs::read_to_string(&path).context("Failed to read config.ini")?;
        Ok(parse_ini(&content))
    }();

    load.unwrap_or_else(|e| {
        eprintln!("Warning: {e:#}; using default config.");
        Config::default()
    })
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::tmp_path;

    #[test]
    fn parses_bare_key_value_pairs() {
        let c = parse_ini("session_cookie=abc\nremember_user_token=xyz\nuser_slug=me\n");
        assert_eq!(c.session_cookie, "abc");
        assert_eq!(c.remember_user_token, "xyz");
        assert_eq!(c.user_slug, "me");
        assert!(!c.debug);
    }

    #[test]
    fn parses_numeric_fields() {
        let c = parse_ini(
            "finish_threshold=99.5\nprogress_step=2.0\npoll_interval=600\nmatch_threshold=0.8\nmin_progress=2.0\n",
        );
        assert_eq!(c.finish_threshold, 99.5);
        assert_eq!(c.progress_step, 2.0);
        assert_eq!(c.poll_interval, 600);
        assert_eq!(c.match_threshold, 0.8);
        assert_eq!(c.min_progress, 2.0);
    }

    #[test]
    fn parses_manual_mappings_path() {
        let c = parse_ini("manual_mappings_path=/tmp/mappings.ini\n");
        assert_eq!(c.manual_mappings_path, "/tmp/mappings.ini");
    }

    #[test]
    fn parses_manual_mappings_skipping_junk() {
        let m = parse_manual_mappings(
            "# comment\n; also comment\n\nKEY1 = id-1\nKEY2=id-2\njunkline\nKEY3=\n",
        );
        assert_eq!(m.len(), 2);
        assert_eq!(m["KEY1"], "id-1");
        assert_eq!(m["KEY2"], "id-2");
    }

    #[test]
    fn parses_sync_scope_defaulting_to_allowlist() {
        assert!(!parse_ini("").sync_all());
        assert!(!parse_ini("sync_scope=allowlist\n").sync_all());
        assert!(parse_ini("sync_scope=all\n").sync_all());
        assert!(parse_ini("sync_scope=ALL\n").sync_all());
    }

    #[test]
    fn parses_allowlist_ignoring_comments_and_suffixes() {
        let a = parse_allowlist("# c\n; c\n\nB0123\nB0456=1\n\n");
        assert_eq!(a.len(), 2);
        assert!(a.contains("B0123"));
        assert!(a.contains("B0456"));
    }

    #[test]
    fn toggle_allowlist_adds_then_removes() {
        let dir = std::env::temp_dir();
        let path = dir.join("ksg_test_allowlist.ini");
        fs::write(&path, "# keep\nB0123\n").unwrap();

        assert!(!toggle_allowlist(&path, "B0123").unwrap()); // removed
        assert_eq!(load_allowlist(&path).len(), 0);

        assert!(toggle_allowlist(&path, "B0123").unwrap()); // re-added
        assert!(load_allowlist(&path).contains("B0123"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_from_allowlist_keeps_comments_and_reports_presence() {
        let path = std::env::temp_dir().join(format!(
            "ksg_test_allowlist_remove_{}.ini",
            std::process::id()
        ));
        fs::write(&path, "# my books\nB0001\nB0002=1\n").unwrap();

        assert!(remove_from_allowlist(&path, "B0002").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# my books\nB0001\n");
        // Absent key: nothing to do, the file is left as is.
        assert!(!remove_from_allowlist(&path, "B0002").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# my books\nB0001\n");
        assert!(!tmp_path(&path).exists(), "temp file must be renamed away");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn add_manual_mapping_adds_then_replaces() {
        let dir = std::env::temp_dir();
        let path = dir.join("ksg_test_mappings.ini");
        fs::write(&path, "# keep\nKEY1=id-1\n").unwrap();

        add_manual_mapping(&path, "KEY2", "id-2").unwrap();
        let m = load_manual_mappings(&path);
        assert_eq!(m["KEY1"], "id-1");
        assert_eq!(m["KEY2"], "id-2");

        add_manual_mapping(&path, "KEY1", "id-9").unwrap();
        let m = load_manual_mappings(&path);
        assert_eq!(m["KEY1"], "id-9");
        assert_eq!(m["KEY2"], "id-2");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn toggle_allowlist_writes_atomically_and_keeps_comments() {
        let path = std::env::temp_dir().join(format!(
            "ksg_test_allowlist_atomic_{}.ini",
            std::process::id()
        ));
        fs::write(&path, "# my books\n; second comment\nB0001\nB0002=1\n").unwrap();

        assert!(toggle_allowlist(&path, "B0003").unwrap());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# my books\n; second comment\nB0001\nB0002=1\nB0003\n"
        );
        assert!(!toggle_allowlist(&path, "B0001").unwrap());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# my books\n; second comment\nB0002=1\nB0003\n"
        );
        assert!(!tmp_path(&path).exists(), "temp file must be renamed away");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn add_manual_mapping_writes_atomically_and_keeps_comments() {
        let path = std::env::temp_dir().join(format!(
            "ksg_test_mappings_atomic_{}.ini",
            std::process::id()
        ));
        fs::write(&path, "# cde_key = storygraph_id\nKEY1=id-1\nKEY2=id-2\n").unwrap();

        add_manual_mapping(&path, "KEY1", "id-9").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# cde_key = storygraph_id\nKEY2=id-2\nKEY1=id-9\n"
        );
        assert!(!tmp_path(&path).exists(), "temp file must be renamed away");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn set_ini_keys_writes_atomically_and_keeps_comments() {
        let path =
            std::env::temp_dir().join(format!("ksg_test_set_ini_{}.ini", std::process::id()));
        fs::write(&path, "# config\nuser_slug=old\ndebug=false\n").unwrap();

        set_ini_keys(&path, &[("user_slug", "new"), ("language", "de")]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# config\nuser_slug=new\ndebug=false\nlanguage=de\n"
        );
        assert!(!tmp_path(&path).exists(), "temp file must be renamed away");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn invalid_numeric_falls_back_to_default() {
        let c = parse_ini("finish_threshold=notanumber\npoll_interval=abc\n");
        assert_eq!(c.finish_threshold, 99.5);
        assert_eq!(c.poll_interval, 900);
    }

    #[test]
    fn ignores_comments_blank_and_section_header() {
        let c = parse_ini(
            "[General]\n# comment\n; also comment\n\nsession_cookie=abc\nunknown_key=ignored\n",
        );
        assert_eq!(c.session_cookie, "abc");
    }

    #[test]
    fn parses_debug_flag() {
        assert!(parse_ini("debug=true\n").debug);
        assert!(parse_ini("debug=1\n").debug);
        assert!(!parse_ini("debug=false\n").debug);
    }

    #[test]
    fn missing_fields_fall_back_to_default() {
        let c = parse_ini("session_cookie=abc\n");
        assert_eq!(c.session_cookie, "abc");
        assert!(c.remember_user_token.is_empty());
        assert_eq!(c.cc_db_path, "/var/local/cc.db");
    }
}
