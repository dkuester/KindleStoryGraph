use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;

use crate::config::remove_from_allowlist;
use crate::error::UserError;
use crate::reader::{BookProgress, Status};
use crate::state::{BookState, State, StateLock};
use crate::storygraph::{self, Book, ReadingStatus, UserBook};

/// Sync tuning knobs (the subset of config relevant to one sync pass).
#[derive(Debug, Clone, Copy)]
pub struct SyncConfig {
    pub progress_step: f64,
    pub match_threshold: f32,
}

/// Abstraction over the StoryGraph client so the sync logic is testable
/// without live HTTP.
pub trait StoryGraphClient {
    fn search(&self, query: &str) -> Result<Vec<Book>>;
    fn user_book(&self, book_id: &str) -> Result<UserBook>;
    fn set_status(&self, book_id: &str, status_id: u8) -> Result<UserBook>;
    fn set_progress(&self, book_id: &str, percent: u32) -> Result<UserBook>;
    /// Remove the book from the user's lists entirely (deletes the reading
    /// history and any review).
    fn remove_book(&self, book_id: &str) -> Result<()>;
}

/// The real client, delegating to the `storygraph` module.
pub struct RealStoryGraph;

impl StoryGraphClient for RealStoryGraph {
    fn search(&self, query: &str) -> Result<Vec<Book>> {
        storygraph::search(query)
    }
    fn user_book(&self, book_id: &str) -> Result<UserBook> {
        storygraph::get_user_book(book_id)
    }
    fn set_status(&self, book_id: &str, status_id: u8) -> Result<UserBook> {
        storygraph::update_status(book_id, status_id)
    }
    fn set_progress(&self, book_id: &str, percent: u32) -> Result<UserBook> {
        storygraph::update_progress(book_id, "percentage", percent)
    }
    fn remove_book(&self, book_id: &str) -> Result<()> {
        storygraph::remove_book(book_id)
    }
}

/// What happened to a single book during a sync pass.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Outcome {
    Attached {
        cde_key: String,
        storygraph_id: String,
        percent: u32,
    },
    Mapped {
        cde_key: String,
        storygraph_id: String,
        percent: u32,
    },
    ProgressUpdated {
        cde_key: String,
        percent: u32,
    },
    Finished {
        cde_key: String,
    },
    /// The Kindle value would move StoryGraph backwards — the book is already
    /// marked read there, or progress made elsewhere is further along — so
    /// nothing was pushed.
    SkippedBehind {
        cde_key: String,
        storygraph_id: String,
        percent: u32,
        remote_percent: u32,
        remote_read: bool,
    },
    NoMatch {
        cde_key: String,
    },
    Unchanged {
        cde_key: String,
    },
    /// This book failed (network, parse, expired session); the pass went on
    /// with the remaining books.
    Failed {
        cde_key: String,
        error: String,
    },
}

impl Outcome {
    /// Whether the outcome changed something on StoryGraph or in the state
    /// (as opposed to a no-op, a skip or a failure).
    pub fn is_change(&self) -> bool {
        matches!(
            self,
            Outcome::Attached { .. }
                | Outcome::Mapped { .. }
                | Outcome::ProgressUpdated { .. }
                | Outcome::Finished { .. }
        )
    }
}

fn status_id(s: Status) -> u8 {
    match s {
        Status::CurrentlyReading => ReadingStatus::CurrentlyReading as u8,
        Status::Read => ReadingStatus::Read as u8,
    }
}

fn status_slug(s: Status) -> &'static str {
    match s {
        Status::CurrentlyReading => "currently-reading",
        Status::Read => "read",
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn percent_int(p: f64) -> u32 {
    p.round().clamp(0.0, 100.0) as u32
}

fn remote_is_read(remote: &UserBook) -> bool {
    remote.status_id == Some(ReadingStatus::Read as u8)
}

/// Backward-progress guard: pushing `percent` for a book in progress would
/// regress StoryGraph when the book is already marked read there (a progress
/// push would also flip it to "rereading") or when progress made on another
/// device is further along.
fn remote_is_ahead(remote: &UserBook, percent: u32) -> bool {
    remote_is_read(remote) || remote.progress_percent > percent
}

/// Find the best StoryGraph match for a Kindle book, or `None` if no
/// candidate's title clears the threshold.
///
/// Only candidates whose title score alone clears `threshold` are eligible;
/// those are ranked by [`storygraph::match_score`] (title score adjusted by
/// author agreement), so of two same-title editions the one by the Kindle
/// author wins. The author thus only reorders candidates: it never rescues a
/// weak title nor rejects a lone title match (e.g. when StoryGraph credits a
/// translator or spells the name differently).
///
/// The search query is the cleaned title only, unlike the human pick list in
/// `menu::find_matches`: the title-only result set already contains the
/// same-title editions to choose between, while appending the Kindle credit
/// (often several authors, or "Last, First") risks StoryGraph returning no
/// results at all for a book it does have.
fn match_book<SG: StoryGraphClient>(
    sg: &SG,
    book: &BookProgress,
    threshold: f32,
) -> Result<Option<String>> {
    let query = storygraph::clean_title(&book.title);
    let results = sg.search(query)?;
    let mut best: Option<(String, f32)> = None;
    for r in &results {
        if storygraph::title_match_score(query, &r.title) < threshold {
            continue;
        }
        let score = storygraph::match_score(query, &book.author, &r.title, &r.author);
        if best.as_ref().is_none_or(|(_, s)| score > *s) {
            best = Some((r.id.clone(), score));
        }
    }
    Ok(best.map(|(id, _)| id))
}

/// Runs one sync pass: computes the diff between the cc.db snapshot and the
/// persisted state, pushes changes to StoryGraph, and mutates `state` in place.
///
/// `manual_mappings` pins `cde_key → storygraph_id` and overrides both the
/// persisted state and the automatic title search (used for books that can't be
/// matched automatically).
///
/// With `dry_run` set, matching/search and the read-only guard lookups still
/// run but nothing is written to StoryGraph and `state` is left untouched —
/// the outcomes show what *would* happen.
///
/// A failing book is reported as [`Outcome::Failed`] and does not abort the
/// pass: the books before it keep their state updates (so the caller can save
/// them) and the books after it are still synced.
pub fn sync_once<SG: StoryGraphClient>(
    snapshot: &[BookProgress],
    state: &mut State,
    config: SyncConfig,
    sg: &SG,
    manual_mappings: &HashMap<String, String>,
    dry_run: bool,
) -> Vec<Outcome> {
    snapshot
        .iter()
        .map(|book| {
            sync_book(book, state, config, sg, manual_mappings, dry_run).unwrap_or_else(|e| {
                Outcome::Failed {
                    cde_key: book.cde_key.clone(),
                    error: format!("{e:#}"),
                }
            })
        })
        .collect()
}

/// Syncs a single book. `state` is only mutated after the StoryGraph writes
/// for that book succeeded, so an error leaves the book's state untouched and
/// the next pass retries it.
fn sync_book<SG: StoryGraphClient>(
    book: &BookProgress,
    state: &mut State,
    config: SyncConfig,
    sg: &SG,
    manual_mappings: &HashMap<String, String>,
    dry_run: bool,
) -> Result<Outcome> {
    // Resolve the StoryGraph id: manual mapping wins, then the persisted
    // state, then an automatic title search.
    let mapped = manual_mappings.get(&book.cde_key).cloned();
    let in_state = state.books.get(&book.cde_key).cloned();

    let resolved: Option<String> = match (&mapped, &in_state) {
        (Some(id), _) => Some(id.clone()),
        (None, Some(bs)) => Some(bs.storygraph_id.clone()),
        (None, None) => match_book(sg, book, config.match_threshold)?,
    };

    let Some(storygraph_id) = resolved else {
        return Ok(Outcome::NoMatch {
            cde_key: book.cde_key.clone(),
        });
    };
    let pct = percent_int(book.percent);

    // Already attached to exactly this id → normal diff handling below.
    let already_attached = in_state
        .as_ref()
        .is_some_and(|bs| bs.storygraph_id == storygraph_id);

    if !already_attached {
        // First observation, or (re)mapped to a different id → attach. A book
        // in progress must not demote a book that is already read on
        // StoryGraph, nor rewind progress made on another device.
        if book.status == Status::CurrentlyReading {
            let remote = sg.user_book(&storygraph_id)?;
            if remote_is_ahead(&remote, pct) {
                if !dry_run {
                    // Remember the link (no re-search next pass) and mirror
                    // the remote status so the diff below stays consistent.
                    let status = if remote_is_read(&remote) {
                        "read"
                    } else {
                        "currently-reading"
                    };
                    state.books.insert(
                        book.cde_key.clone(),
                        BookState {
                            storygraph_id: storygraph_id.clone(),
                            status: status.to_owned(),
                            last_percent: book.percent,
                            last_synced_at: now_epoch(),
                        },
                    );
                }
                return Ok(skipped_behind(book, storygraph_id, pct, &remote));
            }
        }

        if !dry_run {
            sg.set_status(&storygraph_id, status_id(book.status))?;
            if book.status == Status::CurrentlyReading {
                sg.set_progress(&storygraph_id, pct)?;
            }
            state.books.insert(
                book.cde_key.clone(),
                BookState {
                    storygraph_id: storygraph_id.clone(),
                    status: status_slug(book.status).to_owned(),
                    last_percent: book.percent,
                    last_synced_at: now_epoch(),
                },
            );
        }
        return Ok(if mapped.is_some() {
            Outcome::Mapped {
                cde_key: book.cde_key.clone(),
                storygraph_id,
                percent: pct,
            }
        } else {
            Outcome::Attached {
                cde_key: book.cde_key.clone(),
                storygraph_id,
                percent: pct,
            }
        });
    }

    // Already attached to this id → diff against the persisted progress.
    let bs = in_state.expect("already_attached implies a state entry");
    let unchanged = Outcome::Unchanged {
        cde_key: book.cde_key.clone(),
    };
    match book.status {
        Status::Read if bs.status != "read" => {
            if !dry_run {
                // Status only: a progress push on a book that is already read
                // makes StoryGraph's update-progress flow flip it to
                // "rereading".
                sg.set_status(&bs.storygraph_id, ReadingStatus::Read as u8)?;
                if let Some(b) = state.books.get_mut(&book.cde_key) {
                    b.status = "read".to_owned();
                    b.last_percent = book.percent;
                    b.last_synced_at = now_epoch();
                }
            }
            Ok(Outcome::Finished {
                cde_key: book.cde_key.clone(),
            })
        }
        Status::Read => Ok(unchanged),
        Status::CurrentlyReading => {
            if (book.percent - bs.last_percent).abs() < config.progress_step {
                return Ok(unchanged);
            }
            let remote = sg.user_book(&bs.storygraph_id)?;
            if remote_is_ahead(&remote, pct) {
                if !dry_run && let Some(b) = state.books.get_mut(&book.cde_key) {
                    // Track the Kindle value so the guard lookup only repeats
                    // once the Kindle moves by another progress step.
                    b.last_percent = book.percent;
                    if remote_is_read(&remote) {
                        b.status = "read".to_owned();
                    }
                }
                return Ok(skipped_behind(book, bs.storygraph_id.clone(), pct, &remote));
            }
            if !dry_run {
                sg.set_progress(&bs.storygraph_id, pct)?;
                if let Some(b) = state.books.get_mut(&book.cde_key) {
                    b.last_percent = book.percent;
                    b.last_synced_at = now_epoch();
                }
            }
            Ok(Outcome::ProgressUpdated {
                cde_key: book.cde_key.clone(),
                percent: pct,
            })
        }
    }
}

fn skipped_behind(
    book: &BookProgress,
    storygraph_id: String,
    percent: u32,
    remote: &UserBook,
) -> Outcome {
    Outcome::SkippedBehind {
        cde_key: book.cde_key.clone(),
        storygraph_id,
        percent,
        remote_percent: remote.progress_percent,
        remote_read: remote_is_read(remote),
    }
}

/// The allowlist gate in front of a sync pass: with `sync_all` every book is
/// synced, otherwise only books whose `cde_key` is allowlisted.
pub fn allowed_books(
    snapshot: Vec<BookProgress>,
    allowlist: &HashSet<String>,
    sync_all: bool,
) -> Vec<BookProgress> {
    if sync_all {
        snapshot
    } else {
        snapshot
            .into_iter()
            .filter(|b| allowlist.contains(&b.cde_key))
            .collect()
    }
}

/// How [`unsync_book`] takes a book off StoryGraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsyncMode {
    /// Reset the status to "to-read". The book stays on the to-read list and
    /// StoryGraph's reading journal keeps the history (verified 2026-10-21).
    ToRead,
    /// StoryGraph's "Remove book": off every list, reading history **and any
    /// review** deleted. Irreversible — only on an explicit user choice.
    Completely,
}

/// Result of [`unsync_book`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unsynced {
    pub cde_key: String,
    pub storygraph_id: String,
    pub mode: UnsyncMode,
    /// Whether the book was on the allowlist (and has been removed from it).
    pub removed_from_allowlist: bool,
}

/// Takes a synced book back off the user's StoryGraph shelf ("Remove from
/// StoryGraph"): resets its StoryGraph status to "to-read" or, with
/// [`UnsyncMode::Completely`], removes it from StoryGraph entirely (history
/// and review deleted); then removes it from the allowlist (so the next pass
/// does not re-attach it) and from the sync state.
///
/// Order matters for failure: the StoryGraph call comes first and the local
/// files are only touched once it succeeded, so a network error leaves
/// allowlist and state exactly as they were (the user can simply retry). The
/// whole cycle runs under the [`StateLock`], so a concurrent sync pass can
/// neither re-attach the book in between nor overwrite the state save.
///
/// The StoryGraph id comes from `state.json`, i.e. the book this tool
/// actually pushed. A book that only has a manual mapping was never synced:
/// setting it to "to-read" would *add* it to the shelf, so that is refused
/// like an unknown book. The manual mapping itself is kept on purpose, so
/// re-enabling the book later links the same StoryGraph edition again.
///
/// With `sync_all` (`sync_scope=all`) the allowlist does not gate anything
/// and the next pass would put the book straight back — refused up front,
/// before anything changes.
pub fn unsync_book<SG: StoryGraphClient>(
    sg: &SG,
    cde_key: &str,
    state_path: &Path,
    allowlist_path: &Path,
    sync_all: bool,
    mode: UnsyncMode,
) -> Result<Unsynced> {
    let cde_key = cde_key.trim();
    if sync_all {
        return Err(UserError(
            "sync_scope=all syncs every book, so the next pass would put this one straight back \
             on StoryGraph. Set sync_scope=allowlist in config.ini first."
                .to_owned(),
        )
        .into());
    }

    let _lock = StateLock::acquire(state_path)?;
    let mut state = State::load(state_path)?;
    let Some(storygraph_id) = state.books.get(cde_key).map(|b| b.storygraph_id.clone()) else {
        return Err(UserError(format!(
            "'{cde_key}' has not been synced to StoryGraph — nothing to remove."
        ))
        .into());
    };

    match mode {
        UnsyncMode::ToRead => {
            sg.set_status(&storygraph_id, ReadingStatus::ToRead as u8)?;
        }
        UnsyncMode::Completely => sg.remove_book(&storygraph_id)?,
    }

    // Allowlist before state: if the state save failed afterwards, the stale
    // entry is harmless (the book is no longer synced), whereas the reverse
    // would let the next pass re-attach the book.
    let removed_from_allowlist = remove_from_allowlist(allowlist_path, cde_key)?;
    state.books.remove(cde_key);
    state.save(state_path)?;

    Ok(Unsynced {
        cde_key: cde_key.to_owned(),
        storygraph_id,
        mode,
        removed_from_allowlist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Mock {
        results: HashMap<String, Vec<Book>>,
        /// StoryGraph-side `(status_id, progress_percent)` per book id; a
        /// missing entry is a book not on the user's shelf yet.
        remote: HashMap<String, (Option<u8>, u32)>,
        /// Book ids whose writes fail (simulated network/parse error).
        failing: Vec<String>,
        status_calls: RefCell<Vec<(String, u8)>>,
        progress_calls: RefCell<Vec<(String, u32)>>,
        remove_calls: RefCell<Vec<String>>,
    }

    impl Mock {
        fn fail_if_failing(&self, book_id: &str) -> Result<()> {
            if self.failing.iter().any(|f| f == book_id) {
                anyhow::bail!("simulated failure for {book_id}");
            }
            Ok(())
        }
    }

    impl StoryGraphClient for Mock {
        fn search(&self, query: &str) -> Result<Vec<Book>> {
            Ok(self.results.get(query).cloned().unwrap_or_default())
        }
        fn user_book(&self, book_id: &str) -> Result<storygraph::UserBook> {
            self.fail_if_failing(book_id)?;
            let (status_id, progress_percent) =
                self.remote.get(book_id).copied().unwrap_or((None, 0));
            Ok(storygraph::UserBook {
                book_id: book_id.to_owned(),
                status_id,
                progress_percent,
                progress_pages: 0,
                book_pages: 0,
                progress_type: "percentage".to_owned(),
                edition_format: String::new(),
            })
        }
        fn set_status(&self, book_id: &str, status_id: u8) -> Result<storygraph::UserBook> {
            self.fail_if_failing(book_id)?;
            self.status_calls
                .borrow_mut()
                .push((book_id.to_owned(), status_id));
            Ok(storygraph::UserBook {
                book_id: book_id.to_owned(),
                status_id: Some(status_id),
                progress_percent: 0,
                progress_pages: 0,
                book_pages: 0,
                progress_type: "percentage".to_owned(),
                edition_format: String::new(),
            })
        }
        fn set_progress(&self, book_id: &str, percent: u32) -> Result<storygraph::UserBook> {
            self.fail_if_failing(book_id)?;
            self.progress_calls
                .borrow_mut()
                .push((book_id.to_owned(), percent));
            Ok(storygraph::UserBook {
                book_id: book_id.to_owned(),
                status_id: Some(2),
                progress_percent: percent,
                progress_pages: 0,
                book_pages: 0,
                progress_type: "percentage".to_owned(),
                edition_format: String::new(),
            })
        }
        fn remove_book(&self, book_id: &str) -> Result<()> {
            self.fail_if_failing(book_id)?;
            self.remove_calls.borrow_mut().push(book_id.to_owned());
            Ok(())
        }
    }

    fn book(cde_key: &str, title: &str, percent: f64, status: Status) -> BookProgress {
        BookProgress {
            cde_key: cde_key.to_owned(),
            cde_type: "EBOK".to_owned(),
            title: title.to_owned(),
            author: "Some Author".to_owned(),
            percent,
            last_access: 0,
            status,
        }
    }

    fn mock_with(title: &str, id: &str) -> Mock {
        let mut results = HashMap::new();
        results.insert(
            title.to_owned(),
            vec![Book {
                id: id.to_owned(),
                title: title.to_owned(),
                author: "Author".to_owned(),
                pages: 0,
            }],
        );
        Mock {
            results,
            remote: HashMap::new(),
            failing: vec![],
            status_calls: RefCell::new(vec![]),
            progress_calls: RefCell::new(vec![]),
            remove_calls: RefCell::new(vec![]),
        }
    }

    const CFG: SyncConfig = SyncConfig {
        progress_step: 1.0,
        match_threshold: 0.6,
    };

    #[test]
    fn first_observation_attaches_and_sets_progress() {
        let mut state = State::default();
        let mock = mock_with("The Great Gatsby", "sg-1");
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            50.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::Attached {
                cde_key: "asin1".to_owned(),
                storygraph_id: "sg-1".to_owned(),
                percent: 50,
            }]
        );
        assert_eq!(*mock.status_calls.borrow(), vec![("sg-1".to_owned(), 2)]);
        assert_eq!(*mock.progress_calls.borrow(), vec![("sg-1".to_owned(), 50)]);
        assert_eq!(state.books["asin1"].storygraph_id, "sg-1");
        assert_eq!(state.books["asin1"].status, "currently-reading");
    }

    #[test]
    fn delta_below_step_is_skipped() {
        let mut state = State::default();
        state.books.insert(
            "asin1".to_owned(),
            BookState {
                storygraph_id: "sg-1".to_owned(),
                status: "currently-reading".to_owned(),
                last_percent: 50.0,
                last_synced_at: 0,
            },
        );
        let mock = mock_with("The Great Gatsby", "sg-1");
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            50.5,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::Unchanged {
                cde_key: "asin1".to_owned()
            }]
        );
        assert!(mock.progress_calls.borrow().is_empty());
    }

    #[test]
    fn delta_at_or_above_step_updates_progress() {
        let mut state = State::default();
        state.books.insert(
            "asin1".to_owned(),
            BookState {
                storygraph_id: "sg-1".to_owned(),
                status: "currently-reading".to_owned(),
                last_percent: 50.0,
                last_synced_at: 0,
            },
        );
        let mock = mock_with("The Great Gatsby", "sg-1");
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            51.2,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::ProgressUpdated {
                cde_key: "asin1".to_owned(),
                percent: 51,
            }]
        );
        assert_eq!(*mock.progress_calls.borrow(), vec![("sg-1".to_owned(), 51)]);
        assert_eq!(state.books["asin1"].last_percent, 51.2);
    }

    #[test]
    fn crossing_finish_sets_read() {
        let mut state = State::default();
        state.books.insert(
            "asin1".to_owned(),
            BookState {
                storygraph_id: "sg-1".to_owned(),
                status: "currently-reading".to_owned(),
                last_percent: 90.0,
                last_synced_at: 0,
            },
        );
        let mock = mock_with("The Great Gatsby", "sg-1");
        let snapshot = vec![book("asin1", "The Great Gatsby", 99.9, Status::Read)];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::Finished {
                cde_key: "asin1".to_owned()
            }]
        );
        assert_eq!(*mock.status_calls.borrow(), vec![("sg-1".to_owned(), 3)]);
        // No progress push: on a read book it would flip it to "rereading".
        assert!(mock.progress_calls.borrow().is_empty());
        assert_eq!(state.books["asin1"].status, "read");
    }

    #[test]
    fn no_confident_match_is_skipped() {
        let mut state = State::default();
        let mock = mock_with("", ""); // no search results
        let snapshot = vec![book(
            "asin1",
            "Unknown Book",
            50.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::NoMatch {
                cde_key: "asin1".to_owned()
            }]
        );
        assert!(state.books.is_empty());
    }

    #[test]
    fn dry_run_reports_plan_without_mutating() {
        let mut state = State::default();
        let mock = mock_with("The Great Gatsby", "sg-1");
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            50.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), true);

        assert_eq!(
            outcomes,
            vec![Outcome::Attached {
                cde_key: "asin1".to_owned(),
                storygraph_id: "sg-1".to_owned(),
                percent: 50,
            }]
        );
        assert!(mock.status_calls.borrow().is_empty());
        assert!(mock.progress_calls.borrow().is_empty());
        assert!(state.books.is_empty());
    }

    #[test]
    fn manual_mapping_is_used_without_search() {
        let mut state = State::default();
        // No search results at all — the book can only be resolved manually.
        let mock = mock_with("", "");
        let snapshot = vec![book(
            "asin1",
            "Some Unidentifiable Title",
            12.0,
            Status::CurrentlyReading,
        )];

        let mut mappings = HashMap::new();
        mappings.insert("asin1".to_owned(), "sg-manual".to_owned());

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &mappings, false);

        assert_eq!(
            outcomes,
            vec![Outcome::Mapped {
                cde_key: "asin1".to_owned(),
                storygraph_id: "sg-manual".to_owned(),
                percent: 12,
            }]
        );
        assert_eq!(
            *mock.status_calls.borrow(),
            vec![("sg-manual".to_owned(), 2)]
        );
        assert_eq!(
            *mock.progress_calls.borrow(),
            vec![("sg-manual".to_owned(), 12)]
        );
        assert_eq!(state.books["asin1"].storygraph_id, "sg-manual");
    }

    #[test]
    fn prefers_single_book_over_box_set() {
        let mut state = State::default();
        let mut results = HashMap::new();
        results.insert(
            "Dungeon Crawler Carl".to_owned(),
            vec![
                Book {
                    id: "boxset".to_owned(),
                    title: "Dungeon Crawler Carl Series 8-Book Collection Set (Book 1-8)"
                        .to_owned(),
                    author: "Matt Dinniman".to_owned(),
                    pages: 0,
                },
                Book {
                    id: "single".to_owned(),
                    title: "Dungeon Crawler Carl".to_owned(),
                    author: "Matt Dinniman".to_owned(),
                    pages: 0,
                },
            ],
        );
        let mock = Mock {
            results,
            remote: HashMap::new(),
            failing: vec![],
            status_calls: RefCell::new(vec![]),
            progress_calls: RefCell::new(vec![]),
            remove_calls: RefCell::new(vec![]),
        };
        let snapshot = vec![book(
            "asin-dcc",
            "Dungeon Crawler Carl: Dungeon Crawler Carl Book 1",
            23.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::Attached {
                cde_key: "asin-dcc".to_owned(),
                storygraph_id: "single".to_owned(),
                percent: 23,
            }]
        );
    }

    fn candidate(id: &str, title: &str, author: &str) -> Book {
        Book {
            id: id.to_owned(),
            title: title.to_owned(),
            author: author.to_owned(),
            pages: 0,
        }
    }

    fn mock_results(query: &str, books: Vec<Book>) -> Mock {
        Mock {
            results: HashMap::from([(query.to_owned(), books)]),
            remote: HashMap::new(),
            failing: vec![],
            status_calls: RefCell::new(vec![]),
            progress_calls: RefCell::new(vec![]),
            remove_calls: RefCell::new(vec![]),
        }
    }

    fn attached_id(outcomes: &[Outcome]) -> Option<&str> {
        match outcomes {
            [Outcome::Attached { storygraph_id, .. }] => Some(storygraph_id),
            _ => None,
        }
    }

    #[test]
    fn same_title_prefers_matching_author() {
        // Wrong-author edition listed first: without the author it would win
        // the tie on title score.
        for order in [false, true] {
            let mut books = vec![
                candidate("wrong", "The Fall", "Albert Camus"),
                candidate("right", "The Fall", "Martin, Michael A."),
            ];
            if order {
                books.reverse();
            }
            let mock = mock_results("The Fall", books);
            let mut snapshot = vec![book("asin1", "The Fall", 30.0, Status::CurrentlyReading)];
            snapshot[0].author = "Andy Mangels, Michael A. Martin".to_owned();

            let outcomes = sync_once(
                &snapshot,
                &mut State::default(),
                CFG,
                &mock,
                &HashMap::new(),
                false,
            );
            assert_eq!(attached_id(&outcomes), Some("right"));
        }
    }

    #[test]
    fn right_author_beats_slightly_better_title() {
        let mock = mock_results(
            "Titan Taking Wing",
            vec![
                candidate("wrong", "Titan Taking Wing", "Someone Else"),
                candidate(
                    "right",
                    "Titan: Taking Wing (Star Trek)",
                    "Michael A. Martin",
                ),
            ],
        );
        let mut snapshot = vec![book(
            "asin1",
            "Titan Taking Wing",
            30.0,
            Status::CurrentlyReading,
        )];
        snapshot[0].author = "Andy Mangels, Michael A. Martin".to_owned();

        let outcomes = sync_once(
            &snapshot,
            &mut State::default(),
            CFG,
            &mock,
            &HashMap::new(),
            false,
        );
        assert_eq!(attached_id(&outcomes), Some("right"));
    }

    #[test]
    fn unknown_author_keeps_title_only_behaviour() {
        // Kindle author missing: best title wins regardless of result authors.
        let mock = mock_results(
            "Dune Messiah",
            vec![
                candidate("partial", "Dune", "Frank Herbert"),
                candidate("exact", "Dune Messiah", "Someone Else"),
            ],
        );
        let mut snapshot = vec![book(
            "asin1",
            "Dune Messiah",
            30.0,
            Status::CurrentlyReading,
        )];
        snapshot[0].author = String::new();
        let outcomes = sync_once(
            &snapshot,
            &mut State::default(),
            CFG,
            &mock,
            &HashMap::new(),
            false,
        );
        assert_eq!(attached_id(&outcomes), Some("exact"));

        // StoryGraph author unknown: no penalty, the title match still clears.
        let mock = mock_results(
            "Dune Messiah",
            vec![candidate("only", "Dune Messiah", "Unknown Author")],
        );
        snapshot[0].author = "Frank Herbert".to_owned();
        let outcomes = sync_once(
            &snapshot,
            &mut State::default(),
            CFG,
            &mock,
            &HashMap::new(),
            false,
        );
        assert_eq!(attached_id(&outcomes), Some("only"));
    }

    #[test]
    fn wrong_author_lone_title_match_still_attaches() {
        let mock = mock_results(
            "Dune Messiah",
            vec![candidate("only", "Dune Messiah", "Someone Else")],
        );
        let mut snapshot = vec![book(
            "asin1",
            "Dune Messiah",
            30.0,
            Status::CurrentlyReading,
        )];
        snapshot[0].author = "Frank Herbert".to_owned();
        let outcomes = sync_once(
            &snapshot,
            &mut State::default(),
            CFG,
            &mock,
            &HashMap::new(),
            false,
        );
        assert_eq!(attached_id(&outcomes), Some("only"));
    }

    #[test]
    fn author_does_not_lift_weak_title_over_threshold() {
        // Title score 0.4 < 0.6: the matching author must not rescue it.
        let mock = mock_results(
            "Dune Messiah",
            vec![candidate(
                "weak",
                "Dune Chronicles Collection",
                "Frank Herbert",
            )],
        );
        let mut snapshot = vec![book(
            "asin1",
            "Dune Messiah",
            30.0,
            Status::CurrentlyReading,
        )];
        snapshot[0].author = "Frank Herbert".to_owned();
        let outcomes = sync_once(
            &snapshot,
            &mut State::default(),
            CFG,
            &mock,
            &HashMap::new(),
            false,
        );
        assert_eq!(
            outcomes,
            vec![Outcome::NoMatch {
                cde_key: "asin1".to_owned()
            }]
        );
    }

    fn attached(id: &str, status: &str, last_percent: f64) -> BookState {
        BookState {
            storygraph_id: id.to_owned(),
            status: status.to_owned(),
            last_percent,
            last_synced_at: 0,
        }
    }

    #[test]
    fn progress_behind_storygraph_is_not_pushed() {
        // Read to 70 % on another device; the Kindle moved from 40 to 45 %.
        let mut state = State::default();
        state.books.insert(
            "asin1".to_owned(),
            attached("sg-1", "currently-reading", 40.0),
        );
        let mut mock = mock_with("The Great Gatsby", "sg-1");
        mock.remote.insert("sg-1".to_owned(), (Some(2), 70));
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            45.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::SkippedBehind {
                cde_key: "asin1".to_owned(),
                storygraph_id: "sg-1".to_owned(),
                percent: 45,
                remote_percent: 70,
                remote_read: false,
            }]
        );
        assert!(mock.progress_calls.borrow().is_empty());
        assert!(mock.status_calls.borrow().is_empty());
        // The Kindle value is tracked, so the guard lookup isn't repeated
        // until the Kindle moves another step.
        assert_eq!(state.books["asin1"].last_percent, 45.0);
    }

    #[test]
    fn progress_ahead_of_storygraph_is_pushed() {
        let mut state = State::default();
        state.books.insert(
            "asin1".to_owned(),
            attached("sg-1", "currently-reading", 40.0),
        );
        let mut mock = mock_with("The Great Gatsby", "sg-1");
        mock.remote.insert("sg-1".to_owned(), (Some(2), 40));
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            45.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::ProgressUpdated {
                cde_key: "asin1".to_owned(),
                percent: 45,
            }]
        );
        assert_eq!(*mock.progress_calls.borrow(), vec![("sg-1".to_owned(), 45)]);
    }

    #[test]
    fn reopened_read_book_is_not_flipped_to_rereading() {
        // Finished and synced as read; reopening at the start drops the
        // Kindle percent. A progress push would make StoryGraph "rereading".
        let mut state = State::default();
        state
            .books
            .insert("asin1".to_owned(), attached("sg-1", "read", 100.0));
        let mut mock = mock_with("The Great Gatsby", "sg-1");
        mock.remote.insert("sg-1".to_owned(), (Some(3), 0));
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            3.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert!(matches!(
            outcomes.as_slice(),
            [Outcome::SkippedBehind {
                remote_read: true,
                ..
            }]
        ));
        assert!(mock.progress_calls.borrow().is_empty());
        assert!(mock.status_calls.borrow().is_empty());
        assert_eq!(state.books["asin1"].status, "read");
    }

    #[test]
    fn attach_does_not_demote_a_book_read_on_storygraph() {
        let mut state = State::default();
        let mut mock = mock_with("The Great Gatsby", "sg-1");
        mock.remote.insert("sg-1".to_owned(), (Some(3), 0));
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            30.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(
            outcomes,
            vec![Outcome::SkippedBehind {
                cde_key: "asin1".to_owned(),
                storygraph_id: "sg-1".to_owned(),
                percent: 30,
                remote_percent: 0,
                remote_read: true,
            }]
        );
        assert!(mock.status_calls.borrow().is_empty());
        assert!(mock.progress_calls.borrow().is_empty());
        // The link is remembered (no re-search next pass) with the remote
        // status mirrored.
        assert_eq!(state.books["asin1"].storygraph_id, "sg-1");
        assert_eq!(state.books["asin1"].status, "read");
    }

    #[test]
    fn dry_run_guard_does_not_touch_state() {
        let mut state = State::default();
        let mut mock = mock_with("The Great Gatsby", "sg-1");
        mock.remote.insert("sg-1".to_owned(), (Some(2), 80));
        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            30.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), true);

        assert!(matches!(
            outcomes.as_slice(),
            [Outcome::SkippedBehind {
                remote_percent: 80,
                ..
            }]
        ));
        assert!(state.books.is_empty());
    }

    #[test]
    fn failing_book_does_not_abort_the_pass() {
        let mut state = State::default();
        let mut results = HashMap::new();
        for (title, id) in [("Book One", "sg-1"), ("Book Two", "sg-2")] {
            results.insert(
                title.to_owned(),
                vec![Book {
                    id: id.to_owned(),
                    title: title.to_owned(),
                    author: "Author".to_owned(),
                    pages: 0,
                }],
            );
        }
        let mock = Mock {
            results,
            remote: HashMap::new(),
            failing: vec!["sg-1".to_owned()],
            status_calls: RefCell::new(vec![]),
            progress_calls: RefCell::new(vec![]),
            remove_calls: RefCell::new(vec![]),
        };
        let snapshot = vec![
            book("asin1", "Book One", 10.0, Status::CurrentlyReading),
            book("asin2", "Book Two", 20.0, Status::CurrentlyReading),
        ];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &HashMap::new(), false);

        assert_eq!(outcomes.len(), 2);
        assert!(
            matches!(&outcomes[0], Outcome::Failed { cde_key, error }
                if cde_key == "asin1" && error.contains("simulated failure")),
            "unexpected: {:?}",
            outcomes[0]
        );
        assert!(matches!(&outcomes[1], Outcome::Attached { cde_key, .. } if cde_key == "asin2"));
        // The failed book has no state (retried next pass); the other one does.
        assert!(!state.books.contains_key("asin1"));
        assert_eq!(state.books["asin2"].storygraph_id, "sg-2");
    }

    /// Scratch dir with a `state.json` holding `asin1 → sg-1` (and an
    /// unrelated `asin2 → sg-2`) plus an allowlist with both books.
    fn unsync_fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("ksg_unsync_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state_path = dir.join("state.json");
        let allowlist_path = dir.join("allowlist.ini");
        let mut state = State::default();
        state.books.insert(
            "asin1".to_owned(),
            attached("sg-1", "currently-reading", 40.0),
        );
        state
            .books
            .insert("asin2".to_owned(), attached("sg-2", "read", 100.0));
        state.save(&state_path).unwrap();
        std::fs::write(&allowlist_path, "# mine\nasin1\nasin2\n").unwrap();
        (state_path, allowlist_path)
    }

    fn user_error(e: &anyhow::Error) -> &str {
        &e.downcast_ref::<UserError>().expect("a UserError").0
    }

    #[test]
    fn unsync_resets_to_read_and_forgets_the_book() {
        let (state_path, allowlist_path) = unsync_fixture("ok");
        let mock = mock_with("", "");

        let res = unsync_book(
            &mock,
            "asin1",
            &state_path,
            &allowlist_path,
            false,
            UnsyncMode::ToRead,
        )
        .unwrap();

        assert_eq!(
            res,
            Unsynced {
                cde_key: "asin1".to_owned(),
                storygraph_id: "sg-1".to_owned(),
                mode: UnsyncMode::ToRead,
                removed_from_allowlist: true,
            }
        );
        assert!(mock.remove_calls.borrow().is_empty());
        assert_eq!(
            *mock.status_calls.borrow(),
            vec![("sg-1".to_owned(), ReadingStatus::ToRead as u8)]
        );
        assert!(mock.progress_calls.borrow().is_empty());
        let state = State::load(&state_path).unwrap();
        assert!(!state.books.contains_key("asin1"));
        assert_eq!(state.books["asin2"].storygraph_id, "sg-2");
        assert_eq!(
            std::fs::read_to_string(&allowlist_path).unwrap(),
            "# mine\nasin2\n"
        );
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn unsync_unknown_book_is_a_user_error_and_changes_nothing() {
        let (state_path, allowlist_path) = unsync_fixture("unknown");
        let state_before = std::fs::read_to_string(&state_path).unwrap();
        let mock = mock_with("", "");

        let err = unsync_book(
            &mock,
            "nope",
            &state_path,
            &allowlist_path,
            false,
            UnsyncMode::ToRead,
        )
        .unwrap_err();

        assert!(user_error(&err).contains("has not been synced"));
        assert!(mock.status_calls.borrow().is_empty());
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), state_before);
        assert_eq!(
            std::fs::read_to_string(&allowlist_path).unwrap(),
            "# mine\nasin1\nasin2\n"
        );
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn unsync_network_failure_leaves_local_files_untouched() {
        let (state_path, allowlist_path) = unsync_fixture("netfail");
        let state_before = std::fs::read_to_string(&state_path).unwrap();
        let mut mock = mock_with("", "");
        mock.failing.push("sg-1".to_owned());

        let err = unsync_book(
            &mock,
            "asin1",
            &state_path,
            &allowlist_path,
            false,
            UnsyncMode::ToRead,
        )
        .unwrap_err();

        assert!(format!("{err:#}").contains("simulated failure"));
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), state_before);
        assert_eq!(
            std::fs::read_to_string(&allowlist_path).unwrap(),
            "# mine\nasin1\nasin2\n"
        );
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn unsync_is_refused_with_sync_scope_all() {
        let (state_path, allowlist_path) = unsync_fixture("all");
        let state_before = std::fs::read_to_string(&state_path).unwrap();
        let mock = mock_with("", "");

        let err = unsync_book(
            &mock,
            "asin1",
            &state_path,
            &allowlist_path,
            true,
            UnsyncMode::ToRead,
        )
        .unwrap_err();

        assert!(user_error(&err).contains("sync_scope=all"));
        assert!(mock.status_calls.borrow().is_empty());
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), state_before);
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn unsynced_book_is_not_reattached_by_the_next_pass() {
        let (state_path, allowlist_path) = unsync_fixture("reattach");
        // The book is still in progress on the Kindle and would match by title.
        let mock = mock_with("The Great Gatsby", "sg-1");
        // A manual mapping is kept on purpose — it must not re-attach either.
        let mappings = HashMap::from([("asin1".to_owned(), "sg-1".to_owned())]);
        unsync_book(
            &mock,
            "asin1",
            &state_path,
            &allowlist_path,
            false,
            UnsyncMode::ToRead,
        )
        .unwrap();
        mock.status_calls.borrow_mut().clear();

        let snapshot = vec![book(
            "asin1",
            "The Great Gatsby",
            55.0,
            Status::CurrentlyReading,
        )];
        let allowlist = crate::config::load_allowlist(&allowlist_path);
        let snapshot = allowed_books(snapshot, &allowlist, false);
        let mut state = State::load(&state_path).unwrap();
        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, &mappings, false);

        assert!(outcomes.is_empty());
        assert!(mock.status_calls.borrow().is_empty());
        assert!(mock.progress_calls.borrow().is_empty());
        assert!(!state.books.contains_key("asin1"));
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn unsync_completely_removes_the_book_instead_of_to_read() {
        let (state_path, allowlist_path) = unsync_fixture("completely");
        let mock = mock_with("", "");

        let res = unsync_book(
            &mock,
            "asin1",
            &state_path,
            &allowlist_path,
            false,
            UnsyncMode::Completely,
        )
        .unwrap();

        assert_eq!(res.mode, UnsyncMode::Completely);
        assert_eq!(*mock.remove_calls.borrow(), vec!["sg-1".to_owned()]);
        assert!(mock.status_calls.borrow().is_empty(), "no to-read on top");
        let state = State::load(&state_path).unwrap();
        assert!(!state.books.contains_key("asin1"));
        assert_eq!(
            std::fs::read_to_string(&allowlist_path).unwrap(),
            "# mine\nasin2\n"
        );
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }

    #[test]
    fn unsync_completely_failure_leaves_local_files_untouched() {
        let (state_path, allowlist_path) = unsync_fixture("completely_fail");
        let state_before = std::fs::read_to_string(&state_path).unwrap();
        let mut mock = mock_with("", "");
        mock.failing.push("sg-1".to_owned());

        let err = unsync_book(
            &mock,
            "asin1",
            &state_path,
            &allowlist_path,
            false,
            UnsyncMode::Completely,
        )
        .unwrap_err();

        assert!(format!("{err:#}").contains("simulated failure"));
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), state_before);
        assert_eq!(
            std::fs::read_to_string(&allowlist_path).unwrap(),
            "# mine\nasin1\nasin2\n"
        );
        let _ = std::fs::remove_dir_all(state_path.parent().unwrap());
    }
}
