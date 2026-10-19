//! KUAL menu generation and StoryGraph match discovery.

use std::collections::HashMap;
use std::path::Path;

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

/// Build the KUAL `menu.json`: the allowlist selection (with a `[linked]`
/// marker), a "Link books" picker rendered from the cached match candidates,
/// and the standard sync/daemon actions. A missing or unreadable `cc.db` or
/// cache only empties the affected submenus (the menu stays usable).
pub fn build_menu(cc_db: &Path) -> Value {
    let allowlist = config::load_allowlist(Path::new(&CONFIG.allowlist_path));
    let linked = linked_keys(
        Path::new(&CONFIG.manual_mappings_path),
        Path::new(&CONFIG.state_path),
    );
    let cache = SearchCache::load(Path::new(&CONFIG.search_cache_path)).unwrap_or_default();
    let snapshot = Reader::open_read_only(cc_db.to_str().unwrap_or_default())
        .ok()
        .and_then(|reader| {
            reader
                .snapshot(CONFIG.finish_threshold, CONFIG.min_progress)
                .ok()
        })
        .unwrap_or_default();

    let mut selection: Vec<Value> = Vec::new();
    for book in &snapshot {
        let name = if linked.contains_key(&book.cde_key) {
            format!("{}  [linked]", book.title)
        } else {
            book.title.clone()
        };
        selection.push(json!({
            "name": name,
            "checked": allowlist.contains(&book.cde_key),
            "action": "bin/toggle-book.sh",
            "params": book.cde_key,
            "refresh": true,
        }));
    }
    if selection.is_empty() {
        selection.push(json!({
            "name": "(no books with progress yet)",
            "action": "bin/refresh-menu.sh",
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
        "refresh": true,
    }));
    items.push(json!({
        "name": "Refresh book list",
        "priority": 4,
        "action": "bin/refresh-menu.sh",
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

    json!({ "items": items })
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
}
