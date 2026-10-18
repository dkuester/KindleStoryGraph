use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub session_cookie: String,
    pub remember_user_token: String,
    pub user_slug: String,
    pub debug: bool,
    pub cc_db_path: String,
    pub state_path: String,
    pub finish_threshold: f64,
    pub progress_step: f64,
    pub poll_interval: u64,
    pub match_threshold: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            session_cookie: String::new(),
            remember_user_token: String::new(),
            user_slug: String::new(),
            debug: false,
            cc_db_path: "/var/local/cc.db".to_owned(),
            state_path: "/mnt/us/extensions/kindlestorygraph/state.json".to_owned(),
            finish_threshold: 99.5,
            progress_step: 1.0,
            poll_interval: 900,
            match_threshold: 0.6,
        }
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
            "debug" => cfg.debug = value.eq_ignore_ascii_case("true") || value == "1",
            "cc_db_path" => cfg.cc_db_path = value.to_owned(),
            "state_path" => cfg.state_path = value.to_owned(),
            "finish_threshold" => {
                cfg.finish_threshold = value.parse().unwrap_or(cfg.finish_threshold)
            }
            "progress_step" => cfg.progress_step = value.parse().unwrap_or(cfg.progress_step),
            "poll_interval" => cfg.poll_interval = value.parse().unwrap_or(cfg.poll_interval),
            "match_threshold" => cfg.match_threshold = value.parse().unwrap_or(cfg.match_threshold),
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
        eprintln!("Warning: {e:#}; using default config.");
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
    fn parses_numeric_fields() {
        let c = parse_ini(
            "finish_threshold=99.5\nprogress_step=2.0\npoll_interval=600\nmatch_threshold=0.8\n",
        );
        assert_eq!(c.finish_threshold, 99.5);
        assert_eq!(c.progress_step, 2.0);
        assert_eq!(c.poll_interval, 600);
        assert_eq!(c.match_threshold, 0.8);
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
