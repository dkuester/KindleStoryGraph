use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn tmp_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.tmp", path.display()))
}

fn corrupt_archive_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    PathBuf::from(format!("{}.corrupt-{stamp}", path.display()))
}

/// fsync the containing directory so the rename itself is durable. Not
/// supported on vfat (`/mnt/us`), where opening the directory for fsync fails —
/// best effort, failures are ignored on purpose.
fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

impl State {
    /// Loads the state file. A missing or empty file is an empty state.
    ///
    /// A *corrupt* file (torn write after a power cut or suspend) is archived
    /// as `state.json.corrupt-<epoch>` and replaced by an empty state instead
    /// of failing: otherwise every sync cycle would fail at this point and the
    /// sync would stay dead until the file was repaired by hand. The books are
    /// re-attached on the next pass (attaching is idempotent on StoryGraph).
    pub fn load(path: &Path) -> Result<State> {
        if !path.exists() {
            return Ok(State::default());
        }
        let content =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if content.trim().is_empty() {
            return Ok(State::default());
        }
        match serde_json::from_str(&content) {
            Ok(state) => Ok(state),
            Err(e) => {
                let archive = corrupt_archive_path(path);
                fs::rename(path, &archive).with_context(|| {
                    format!(
                        "archiving corrupt state file {} as {}",
                        path.display(),
                        archive.display()
                    )
                })?;
                eprintln!(
                    "state file {} was corrupt ({e}); archived as {}. \
                     Continuing with empty state — books are re-attached on the next sync.",
                    path.display(),
                    archive.display()
                );
                Ok(State::default())
            }
        }
    }

    /// Writes the state file atomically: temp file → fsync → rename. A suspend
    /// or power cut mid-write can then never leave a partial JSON file behind
    /// (worst case the `.tmp` file survives and the previous state stays valid).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir of {}", path.display()))?;
        }
        let tmp = tmp_path(path);
        let json = serde_json::to_vec_pretty(self)?;
        {
            let mut file = File::create(&tmp)
                .with_context(|| format!("creating temp state file {}", tmp.display()))?;
            file.write_all(&json)
                .with_context(|| format!("writing {}", tmp.display()))?;
            // The content has to reach the medium before the rename is visible.
            file.sync_all()
                .with_context(|| format!("flushing {}", tmp.display()))?;
        }
        fs::rename(&tmp, path)
            .with_context(|| format!("replacing {} with {}", path.display(), tmp.display()))?;
        sync_parent_dir(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(entries: &[(&str, &str, f64)]) -> State {
        let mut s = State::default();
        for (key, id, percent) in entries {
            s.books.insert(
                (*key).to_owned(),
                BookState {
                    storygraph_id: (*id).to_owned(),
                    status: "currently-reading".to_owned(),
                    last_percent: *percent,
                    last_synced_at: 1_700_000_000,
                },
            );
        }
        s
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        // Start clean: leftover archives from an earlier run would break the
        // "exactly one archive" assertion.
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_missing_file_is_empty_state() {
        let s = State::load(Path::new("/nonexistent/state.json")).unwrap();
        assert!(s.books.is_empty());
    }

    #[test]
    fn roundtrip_preserves_books() {
        let s = state_with(&[("asin1", "sg-1", 64.59)]);
        let dir = temp_dir("kindlestorygraph-test");
        let path = dir.join("state.json");

        s.save(&path).unwrap();

        let loaded = State::load(&path).unwrap();
        assert_eq!(loaded, s);
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let dir = temp_dir("kindlestorygraph-test-atomic");
        let path = dir.join("state.json");
        state_with(&[("old", "sg-old", 1.0)]).save(&path).unwrap();

        state_with(&[("new", "sg-new", 2.0)]).save(&path).unwrap();

        assert!(!tmp_path(&path).exists(), "temp file must be renamed away");
        // Still valid JSON holding only the new content — never a mix.
        let loaded = State::load(&path).unwrap();
        assert_eq!(loaded.books.len(), 1);
        assert!(loaded.books.contains_key("new"));
    }

    #[test]
    fn corrupt_state_is_archived_and_loads_empty() {
        let dir = temp_dir("kindlestorygraph-test-corrupt");
        let path = dir.join("state.json");
        fs::write(&path, "{\"books\": {\"a\": ").unwrap(); // torn write

        let loaded = State::load(&path).unwrap();
        assert!(loaded.books.is_empty(), "corrupt state must load as empty");
        assert!(!path.exists(), "corrupt file must be moved away");

        let archived: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("state.json.corrupt-")
            })
            .collect();
        assert_eq!(
            archived.len(),
            1,
            "the broken file must be kept for inspection"
        );
    }

    #[test]
    fn empty_state_file_loads_empty() {
        let dir = temp_dir("kindlestorygraph-test-empty");
        let path = dir.join("state.json");
        fs::write(&path, "  \n").unwrap();
        assert!(State::load(&path).unwrap().books.is_empty());
    }
}
