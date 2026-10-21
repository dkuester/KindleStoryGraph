//! Crash-safe file writes for everything the mod owns under `/mnt/us` (vfat).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The temp file `write_atomic` stages `path` in: `<path>.tmp`, in the same
/// directory so the final rename never crosses a filesystem.
pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.tmp", path.display()))
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

/// Replaces `path` with `bytes` atomically: temp file → fsync → rename →
/// best-effort parent-dir fsync. A suspend or power cut mid-write can then
/// never leave a truncated file behind (worst case the `.tmp` file survives
/// and the previous content stays valid). Creates missing parent dirs.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir of {}", path.display()))?;
    }
    let tmp = tmp_path(path);
    {
        let mut file = File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        file.write_all(bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ksg_fsutil_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn write_atomic_creates_parent_and_leaves_no_tmp() {
        let dir = scratch_dir("create");
        let path = dir.join("nested").join("file.json");

        write_atomic(&path, b"{\"a\":1}").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":1}");
        assert!(!tmp_path(&path).exists(), "temp file must be renamed away");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_replaces_longer_content_fully() {
        let dir = scratch_dir("replace");
        let path = dir.join("file.ini");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "a much longer previous content\nwith two lines\n").unwrap();

        write_atomic(&path, b"short\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "short\n");
        assert!(!tmp_path(&path).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_overwrites_stale_tmp_from_interrupted_write() {
        let dir = scratch_dir("stale");
        let path = dir.join("file.ini");
        fs::create_dir_all(&dir).unwrap();
        fs::write(tmp_path(&path), "torn garbage from a power cut").unwrap();

        write_atomic(&path, b"fresh\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh\n");
        assert!(!tmp_path(&path).exists());
        fs::remove_dir_all(&dir).ok();
    }
}
