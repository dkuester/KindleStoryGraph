use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Per-book sync state, keyed by Kindle `p_cdeKey`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookState {
    pub storygraph_id: String,
    /// StoryGraph status slug: "currently-reading" | "read".
    pub status: String,
    /// Last percent we pushed (raw float from cc.db).
    pub last_percent: f64,
    /// Epoch seconds of the last successful sync for this book.
    pub last_synced_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct State {
    pub books: HashMap<String, BookState>,
}

impl State {
    pub fn load(path: &Path) -> Result<State> {
        if !path.exists() {
            return Ok(State::default());
        }
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if content.trim().is_empty() {
            return Ok(State::default());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_is_empty_state() {
        let s = State::load(Path::new("/nonexistent/state.json")).unwrap();
        assert!(s.books.is_empty());
    }

    #[test]
    fn roundtrip_preserves_books() {
        let mut s = State::default();
        s.books.insert(
            "asin1".to_owned(),
            BookState {
                storygraph_id: "sg-1".to_owned(),
                status: "currently-reading".to_owned(),
                last_percent: 64.59,
                last_synced_at: 1234567890,
            },
        );

        let dir = std::env::temp_dir().join("kindlestorygraph-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        s.save(&path).unwrap();

        let loaded = State::load(&path).unwrap();
        assert_eq!(loaded, s);
    }
}
