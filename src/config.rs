use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub session_cookie: String,
    pub remember_user_token: String,
    pub user_slug: String,
    pub debug: bool,
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
            "debug" => cfg.debug = value.eq_ignore_ascii_case("true") || value == "1",
            _ => {}
        }
    }
    cfg
}

/// Resolves the config path: the `KINDLE_STORYGRAPH_CONFIG` env var if set
/// (handy for host testing), otherwise `config.ini` next to the binary.
fn config_path() -> Result<PathBuf> {
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
        eprintln!("Warning: {e:#}; using empty config.");
        Config::default()
    })
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_key_value_pairs() {
        let c = parse_ini("session_cookie=abc\nremember_user_token=xyz\nuser_slug=me\n");
        assert_eq!(c.session_cookie, "abc");
        assert_eq!(c.remember_user_token, "xyz");
        assert_eq!(c.user_slug, "me");
        assert!(!c.debug);
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
        assert!(c.user_slug.is_empty());
    }
}
