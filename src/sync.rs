use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;

use crate::reader::{BookProgress, Status};
use crate::state::{BookState, State};
use crate::storygraph::{self, Book, ReadingStatus};

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
    fn set_status(&self, book_id: &str, status_id: u8) -> Result<storygraph::UserBook>;
    fn set_progress(&self, book_id: &str, percent: u32) -> Result<storygraph::UserBook>;
}

/// The real client, delegating to the `storygraph` module.
pub struct RealStoryGraph;

impl StoryGraphClient for RealStoryGraph {
    fn search(&self, query: &str) -> Result<Vec<Book>> {
        storygraph::search(query)
    }
    fn set_status(&self, book_id: &str, status_id: u8) -> Result<storygraph::UserBook> {
        storygraph::update_status(book_id, status_id)
    }
    fn set_progress(&self, book_id: &str, percent: u32) -> Result<storygraph::UserBook> {
        storygraph::update_progress(book_id, "percentage", percent)
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
    ProgressUpdated {
        cde_key: String,
        percent: u32,
    },
    Finished {
        cde_key: String,
    },
    NoMatch {
        cde_key: String,
    },
    Unchanged {
        cde_key: String,
    },
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

/// Find the best StoryGraph match for a Kindle book by title, or `None` if no
/// candidate clears the threshold.
fn match_book<SG: StoryGraphClient>(
    sg: &SG,
    book: &BookProgress,
    threshold: f32,
) -> Result<Option<String>> {
    let query = storygraph::clean_title(&book.title);
    let results = sg.search(query)?;
    let mut best: Option<(String, f32)> = None;
    for r in &results {
        let score = storygraph::title_match_score(query, &r.title);
        if best.as_ref().is_none_or(|(_, s)| score > *s) {
            best = Some((r.id.clone(), score));
        }
    }
    Ok(best.filter(|(_, s)| *s >= threshold).map(|(id, _)| id))
}

/// Runs one sync pass: computes the diff between the cc.db snapshot and the
/// persisted state, pushes changes to StoryGraph, and mutates `state` in place.
///
/// With `dry_run` set, matching/search still runs but nothing is written to
/// StoryGraph and `state` is left untouched — the outcomes show what *would*
/// happen.
pub fn sync_once<SG: StoryGraphClient>(
    snapshot: &[BookProgress],
    state: &mut State,
    config: SyncConfig,
    sg: &SG,
    dry_run: bool,
) -> Result<Vec<Outcome>> {
    let mut outcomes = Vec::new();

    for book in snapshot {
        match state.books.get(&book.cde_key).cloned() {
            None => {
                // First observation → attach to StoryGraph.
                let Some(storygraph_id) = match_book(sg, book, config.match_threshold)? else {
                    outcomes.push(Outcome::NoMatch {
                        cde_key: book.cde_key.clone(),
                    });
                    continue;
                };
                if !dry_run {
                    sg.set_status(&storygraph_id, status_id(book.status))?;
                    if book.status == Status::CurrentlyReading {
                        sg.set_progress(&storygraph_id, percent_int(book.percent))?;
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
                outcomes.push(Outcome::Attached {
                    cde_key: book.cde_key.clone(),
                    storygraph_id,
                    percent: percent_int(book.percent),
                });
            }
            Some(bs) => match book.status {
                Status::Read if bs.status != "read" => {
                    if !dry_run {
                        sg.set_status(&bs.storygraph_id, ReadingStatus::Read as u8)?;
                        sg.set_progress(&bs.storygraph_id, percent_int(book.percent))?;
                        if let Some(b) = state.books.get_mut(&book.cde_key) {
                            b.status = "read".to_owned();
                            b.last_percent = book.percent;
                            b.last_synced_at = now_epoch();
                        }
                    }
                    outcomes.push(Outcome::Finished {
                        cde_key: book.cde_key.clone(),
                    });
                }
                Status::CurrentlyReading => {
                    if (book.percent - bs.last_percent).abs() >= config.progress_step {
                        let pct = percent_int(book.percent);
                        if !dry_run {
                            sg.set_progress(&bs.storygraph_id, pct)?;
                            if let Some(b) = state.books.get_mut(&book.cde_key) {
                                b.last_percent = book.percent;
                                b.last_synced_at = now_epoch();
                            }
                        }
                        outcomes.push(Outcome::ProgressUpdated {
                            cde_key: book.cde_key.clone(),
                            percent: pct,
                        });
                    } else {
                        outcomes.push(Outcome::Unchanged {
                            cde_key: book.cde_key.clone(),
                        });
                    }
                }
                Status::Read => outcomes.push(Outcome::Unchanged {
                    cde_key: book.cde_key.clone(),
                }),
            },
        }
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Mock {
        results: HashMap<String, Vec<Book>>,
        status_calls: RefCell<Vec<(String, u8)>>,
        progress_calls: RefCell<Vec<(String, u32)>>,
    }

    impl StoryGraphClient for Mock {
        fn search(&self, query: &str) -> Result<Vec<Book>> {
            Ok(self.results.get(query).cloned().unwrap_or_default())
        }
        fn set_status(&self, book_id: &str, status_id: u8) -> Result<storygraph::UserBook> {
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
            status_calls: RefCell::new(vec![]),
            progress_calls: RefCell::new(vec![]),
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

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, false).unwrap();

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

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, false).unwrap();

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

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, false).unwrap();

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

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, false).unwrap();

        assert_eq!(
            outcomes,
            vec![Outcome::Finished {
                cde_key: "asin1".to_owned()
            }]
        );
        assert_eq!(*mock.status_calls.borrow(), vec![("sg-1".to_owned(), 3)]);
        assert_eq!(
            *mock.progress_calls.borrow(),
            vec![("sg-1".to_owned(), 100)]
        );
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

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, false).unwrap();

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

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, true).unwrap();

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
            status_calls: RefCell::new(vec![]),
            progress_calls: RefCell::new(vec![]),
        };
        let snapshot = vec![book(
            "asin-dcc",
            "Dungeon Crawler Carl: Dungeon Crawler Carl Book 1",
            23.0,
            Status::CurrentlyReading,
        )];

        let outcomes = sync_once(&snapshot, &mut state, CFG, &mock, false).unwrap();

        assert_eq!(
            outcomes,
            vec![Outcome::Attached {
                cde_key: "asin-dcc".to_owned(),
                storygraph_id: "single".to_owned(),
                percent: 23,
            }]
        );
    }
}
