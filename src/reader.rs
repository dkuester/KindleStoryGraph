use std::collections::HashSet;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

/// Percent at or above which a book counts as finished ("read").
pub const DEFAULT_FINISH_THRESHOLD: f64 = 99.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    CurrentlyReading,
    Read,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BookProgress {
    pub cde_key: String,
    pub cde_type: String,
    pub title: String,
    pub author: String,
    pub percent: f64,
    pub last_access: i64,
    pub status: Status,
}

/// Derive a StoryGraph status from a percent value.
///
/// `None` = unread / no progress (ignore).
pub fn derive_status(percent: f64, finish_threshold: f64) -> Option<Status> {
    if percent <= 0.0 {
        None
    } else if percent >= finish_threshold {
        Some(Status::Read)
    } else {
        Some(Status::CurrentlyReading)
    }
}

/// First author display name from a `j_credits` JSON column value.
pub fn author_from_credits(credits_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(credits_json)
        .ok()
        .and_then(|v| {
            v.as_array()?
                .first()?
                .get("name")?
                .get("display")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

struct RawRow {
    cde_key: String,
    cde_type: String,
    title: String,
    credits: String,
    percent: f64,
    last_access: i64,
}

/// Deduplicate by `cde_key` (greatest `last_access` wins) and map to progress
/// entries. `rows` must be sorted by `last_access` descending, so the first
/// occurrence of each key is the winner. Books below `min_progress` are
/// dropped (progress must reach the unlock threshold before syncing).
fn deduplicate(rows: Vec<RawRow>, finish_threshold: f64, min_progress: f64) -> Vec<BookProgress> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for r in rows {
        if !seen.insert(r.cde_key.clone()) {
            continue;
        }
        if r.percent < min_progress {
            continue;
        }
        let Some(status) = derive_status(r.percent, finish_threshold) else {
            continue;
        };
        out.push(BookProgress {
            cde_key: r.cde_key,
            cde_type: r.cde_type,
            title: r.title,
            author: author_from_credits(&r.credits),
            percent: r.percent,
            last_access: r.last_access,
            status,
        });
    }
    out
}

pub struct Reader {
    conn: Connection,
}

impl Reader {
    /// Open a `cc.db` read-only (on-device it is held open by `KPPMainApp`).
    pub fn open_read_only(path: &str) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("opening cc.db at {path}"))?;
        Ok(Reader { conn })
    }

    /// Snapshot of local books with progress, most recent first.
    ///
    /// Filters `p_isArchived = 0` (the cloud-only duplicate rows carry no
    /// progress), `p_percentFinished > 0`, and books below `min_progress`
    /// (a book must reach the unlock threshold before it is synced).
    pub fn snapshot(&self, finish_threshold: f64, min_progress: f64) -> Result<Vec<BookProgress>> {
        let mut stmt = self.conn.prepare(
            "SELECT p_cdeKey, p_cdeType, p_titles_0_nominal, j_credits,
                    p_percentFinished, p_lastAccess
             FROM Entries
             WHERE p_type = 'Entry:Item'
               AND p_isArchived = 0
               AND p_percentFinished IS NOT NULL
               AND p_percentFinished > 0
             ORDER BY p_lastAccess DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(RawRow {
                cde_key: row.get(0)?,
                cde_type: row.get(1)?,
                title: row.get(2)?,
                credits: row.get(3)?,
                percent: row.get(4)?,
                last_access: row.get(5)?,
            })
        })?;

        let mut raw: Vec<RawRow> = rows.collect::<Result<_, _>>()?;
        Ok(deduplicate(
            std::mem::take(&mut raw),
            finish_threshold,
            min_progress,
        ))
    }

    /// The currently-read book: the in-progress book accessed most recently.
    pub fn current_book(
        &self,
        finish_threshold: f64,
        min_progress: f64,
    ) -> Result<Option<BookProgress>> {
        Ok(self
            .snapshot(finish_threshold, min_progress)?
            .into_iter()
            .find(|b| b.status == Status::CurrentlyReading))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    const CREDITS: &str = r#"[{"name":{"display":"Andy Mangels, Michael A. Martin","collation":"Andy Mangels Michael A Martin","language":"de"},"kind":"Author"}]"#;

    fn test_reader() -> Reader {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE Entries (
                p_uuid TEXT PRIMARY KEY,
                p_type TEXT,
                p_isArchived INTEGER,
                p_cdeKey TEXT,
                p_cdeType TEXT,
                p_titles_0_nominal TEXT,
                j_credits TEXT,
                p_percentFinished REAL,
                p_lastAccess INTEGER
            );",
        )
        .unwrap();
        Reader { conn }
    }

    fn insert(
        r: &Reader,
        uuid: &str,
        archived: i64,
        key: &str,
        title: &str,
        percent: Option<f64>,
        last_access: i64,
    ) {
        r.conn
            .execute(
                "INSERT INTO Entries (p_uuid, p_type, p_isArchived, p_cdeKey, p_cdeType,
                                      p_titles_0_nominal, j_credits, p_percentFinished, p_lastAccess)
                 VALUES (?1, 'Entry:Item', ?2, ?3, 'EBOK', ?4, ?5, ?6, ?7)",
                rusqlite::params![uuid, archived, key, title, CREDITS, percent, last_access],
            )
            .unwrap();
    }

    #[test]
    fn status_boundaries() {
        assert_eq!(derive_status(0.0, 99.5), None);
        assert_eq!(derive_status(-1.0, 99.5), None);
        assert_eq!(derive_status(0.5, 99.5), Some(Status::CurrentlyReading));
        assert_eq!(derive_status(99.49, 99.5), Some(Status::CurrentlyReading));
        assert_eq!(derive_status(99.5, 99.5), Some(Status::Read));
        assert_eq!(derive_status(99.86, 99.5), Some(Status::Read));
        assert_eq!(derive_status(100.0, 99.5), Some(Status::Read));
    }

    #[test]
    fn author_extraction() {
        assert_eq!(
            author_from_credits(CREDITS),
            "Andy Mangels, Michael A. Martin"
        );
        assert_eq!(author_from_credits("not json"), "");
        assert_eq!(author_from_credits("[]"), "");
    }

    #[test]
    fn dedup_keeps_most_recent() {
        let rows = vec![
            RawRow {
                cde_key: "K".into(),
                cde_type: "EBOK".into(),
                title: "T".into(),
                credits: CREDITS.into(),
                percent: 70.0,
                last_access: 200,
            },
            RawRow {
                cde_key: "K".into(),
                cde_type: "EBOK".into(),
                title: "T".into(),
                credits: CREDITS.into(),
                percent: 60.0,
                last_access: 100,
            },
        ];
        let out = deduplicate(rows, 99.5, 0.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].percent, 70.0);
        assert_eq!(out[0].last_access, 200);
    }

    #[test]
    fn snapshot_excludes_archived_and_unread() {
        let r = test_reader();
        insert(&r, "a", 0, "LOCAL1", "In progress", Some(64.59), 200);
        // cloud-only duplicate of LOCAL1: archived, no progress -> must be excluded
        insert(&r, "b", 1, "LOCAL1", "In progress (cloud dup)", None, 100);
        insert(&r, "c", 0, "LOCAL2", "Finished", Some(100.0), 150);
        insert(&r, "d", 0, "LOCAL3", "Unread", Some(0.0), 50);

        let snap = r.snapshot(99.5, 0.0).unwrap();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].cde_key, "LOCAL1");
        assert_eq!(snap[0].status, Status::CurrentlyReading);
        assert_eq!(snap[0].percent, 64.59);
        assert_eq!(snap[1].cde_key, "LOCAL2");
        assert_eq!(snap[1].status, Status::Read);

        let cur = r.current_book(99.5, 0.0).unwrap().unwrap();
        assert_eq!(cur.cde_key, "LOCAL1");
        assert_eq!(cur.author, "Andy Mangels, Michael A. Martin");
    }

    #[test]
    fn snapshot_respects_min_progress() {
        let r = test_reader();
        insert(&r, "a", 0, "LOW", "Barely started", Some(1.0), 200);
        insert(&r, "b", 0, "OK", "Actually reading", Some(5.0), 150);

        let snap = r.snapshot(99.5, 2.0).unwrap();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].cde_key, "OK");
        assert_eq!(snap[0].percent, 5.0);
    }
}
