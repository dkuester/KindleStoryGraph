use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::{CONFIG, load_allowlist, load_manual_mappings};
use crate::reader::Reader;
use crate::state::State;
use crate::sync::{self, RealStoryGraph, SyncConfig};

/// Whether the device currently has network. On Kindle this checks `wlan0`;
/// on a host (no `wlan0` sysfs node) it assumes the network is up.
fn network_up() -> bool {
    let oper = std::fs::read_to_string("/sys/class/net/wlan0/operstate").unwrap_or_default();
    if oper.trim().is_empty() {
        return true;
    }
    oper.trim() == "up"
}

/// One sync pass: read cc.db, diff against the persisted state, push changes to
/// StoryGraph, and persist the updated state.
pub fn sync_once_from_paths(
    cc_db: &Path,
    state_path: &Path,
    dry_run: bool,
) -> Result<Vec<sync::Outcome>> {
    let reader = Reader::open_read_only(cc_db.to_str().context("non-UTF-8 path")?)?;
    let snapshot = reader.snapshot(CONFIG.finish_threshold, CONFIG.min_progress)?;
    // Allowlist gate (default): only explicitly allowed books are synced.
    // With `sync_scope=all` every book is synced.
    let snapshot: Vec<_> = if CONFIG.sync_all() {
        snapshot
    } else {
        let allowlist = load_allowlist(Path::new(&CONFIG.allowlist_path));
        snapshot
            .into_iter()
            .filter(|b| allowlist.contains(&b.cde_key))
            .collect()
    };
    let mut state = State::load(state_path)?;
    let sg = RealStoryGraph;
    let cfg = SyncConfig {
        progress_step: CONFIG.progress_step,
        match_threshold: CONFIG.match_threshold,
    };
    let mappings = load_manual_mappings(Path::new(&CONFIG.manual_mappings_path));
    let outcomes = sync::sync_once(&snapshot, &mut state, cfg, &sg, &mappings, dry_run)?;
    if !dry_run {
        state.save(state_path)?;
    }
    Ok(outcomes)
}

/// Runs the sync daemon in a loop. Each cycle syncs once (only when the network
/// is up), then sleeps for `poll_interval_secs`.
pub fn run(poll_interval_secs: u64) -> Result<()> {
    let cc_db = Path::new(&CONFIG.cc_db_path);
    let state_path = Path::new(&CONFIG.state_path);

    loop {
        if network_up() {
            match sync_once_from_paths(cc_db, state_path, false) {
                Ok(outcomes) => {
                    for o in &outcomes {
                        if !matches!(o, sync::Outcome::Unchanged { .. }) {
                            eprintln!("{o:?}");
                        }
                    }
                }
                Err(e) => eprintln!("sync error: {e:#}"),
            }
        }
        sleep(Duration::from_secs(poll_interval_secs));
    }
}
