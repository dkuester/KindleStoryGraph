//! Headless UI tests (stage 3 of the virtual Kindle, docs/emulator.md): the
//! real binary's FBInk UI on a character grid (`ui --script … --frames …`),
//! driven by tap scripts, against the fake StoryGraph from stage 2.
//!
//! The UI exits non-zero when any text runs past the right edge of the
//! screen, so every green run here also proves the screens fit.
//!
//! Golden frames live in `tests/ui/golden/`; after an intended layout change
//! regenerate them with `UPDATE_GOLDEN=1 cargo test --test ui` and review the
//! diff.

mod common;

use std::path::Path;
use std::process::{Command, Stdio};

use common::*;
use serde_json::json;

const PW11: &str = "51x34"; // Paperwhite 11th gen (1236x1648)
const KT: &str = "44x30"; // Kindle 2022/2024 (1072x1448)

/// UI labels per language, so the scripts read like what the user taps.
struct Labels {
    sync: &'static str,
    exit: &'static str,
    settings: &'static str,
    link: &'static str,
    back: &'static str,
    remove: &'static str,
    remove_completely: &'static str,
    delete: &'static str,
    cancel: &'static str,
}

const EN: Labels = Labels {
    sync: "[ Sync now ]",
    exit: "[ Exit ]",
    settings: "[ Settings ]",
    link: "[ Link ]",
    back: "[ Back ]",
    remove: "[ Remove ]",
    remove_completely: "[ Remove completely... ]",
    delete: "[ Delete ]",
    cancel: "[ Cancel ]",
};

const DE: Labels = Labels {
    sync: "[ Sync jetzt ]",
    exit: "[ Beenden ]",
    settings: "[ Einstellungen ]",
    link: "[ Verknüpfen ]",
    back: "[ Zurück ]",
    remove: "[ Entfernen ]",
    remove_completely: "[ Komplett entfernen... ]",
    delete: "[ Löschen ]",
    cancel: "[ Abbrechen ]",
};

/// The settings screen shows the config's paths, i.e. this run's temp dir:
/// blank them out so the golden frames do not depend on the machine.
fn normalize_paths(frames: &str) -> String {
    frames
        .lines()
        .map(|line| {
            let keys = ["cc.db", "state", "manual mappings", "allowlist"];
            match line.strip_prefix("|  ") {
                Some(rest) if keys.iter().any(|k| rest.starts_with(&format!("{k} "))) => {
                    let width = line.chars().count();
                    let head: String = line.chars().take(22).collect();
                    let tail = line.chars().rev().take_while(|c| *c != '|').count();
                    format!(
                        "{head}<temp>{}{}",
                        " ".repeat(width - 22 - 6 - tail - 1),
                        line.chars().skip(width - tail - 1).collect::<String>()
                    )
                }
                _ => line.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_golden(name: &str, actual: &str) {
    let path = Path::new(ROOT).join("tests/ui/golden").join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; run UPDATE_GOLDEN=1 cargo test --test ui",
            path.display()
        )
    });
    assert!(
        expected == actual,
        "{name} differs from the golden file (UPDATE_GOLDEN=1 regenerates it):\n{actual}"
    );
}

/// The frame showing `text`, last one first (the state after an action).
fn last_frame_with<'a>(frames: &'a [String], text: &str) -> &'a str {
    frames
        .iter()
        .rev()
        .find(|f| f.contains(text))
        .unwrap_or_else(|| panic!("no frame shows {text:?}:\n{}", frames.join("\n")))
}

fn assert_ok(run: &Run) {
    assert_eq!(run.code, 0, "ui failed:\n{}\n{}", run.stderr, run.stdout);
}

/// Every screen once: settings, link (search + pick), remove (to-read) with
/// a detour through "Remove completely" and back. Starts with Dune and Der
/// Schwarm synced.
fn tour(l: &Labels) -> String {
    [
        format!("tap {}", l.settings),
        format!("tap {}", l.back),
        format!("tap {}", l.link),
        "tap Der Schwarm".to_owned(),
        "wait".to_owned(),
        "tap Der Schwarm - Frank".to_owned(),
        format!("tap {}", l.link),
        format!("tap {}", l.remove),
        "tap Dune".to_owned(),
        format!("tap {}", l.remove_completely),
        format!("tap {}", l.cancel),
        format!("tap {}", l.remove),
        "wait".to_owned(),
        format!("tap {}", l.exit),
    ]
    .join("\n")
}

#[test]
fn tour_of_every_screen_matches_golden_frames() {
    for (lang, labels, size) in [("en", &EN, PW11), ("de", &DE, KT)] {
        let k = Kindle::new(&format!("tour-{lang}"));
        k.set_config("language", lang);
        k.allow(&[DUNE, SCHWARM]);
        assert_eq!(k.sync().code, 0);

        let (run, frames) = k.ui(&tour(labels), size, &[]);
        assert_ok(&run);
        // Searching / removing frames included: the order is deterministic
        // because the script waits for the background work.
        let all = normalize_paths(&frames.join("\n"));
        // Fixed UI text is never cut ("~"); only the paths may be.
        let cut: Vec<_> = all.lines().filter(|l| l.contains('~')).collect();
        assert!(
            cut.is_empty(),
            "truncated UI text on {size}:\n{}",
            cut.join("\n")
        );
        assert_golden(&format!("tour_{lang}_{size}.txt"), &all);

        // The same flow on the server and on disk.
        assert_eq!(k.fake.shelf("dune-0001")["status"], "to-read");
        assert_eq!(k.fake.shelf("schwarm-0001")["status"], "currently-reading");
        let mappings = std::fs::read_to_string(k.dir.join("manual_mappings.ini")).unwrap();
        assert!(mappings.contains(&format!("{SCHWARM}=schwarm-0001")));
        assert!(!k.allowlist().contains(DUNE));
        assert!(k.state()["books"][DUNE].is_null());
    }
}

/// Regression: "Sync now" put a book on the shelf, but the Remove list kept
/// the rows loaded at start and showed "(no synced books)".
#[test]
fn sync_now_then_remove_in_the_same_session() {
    let k = Kindle::new("sync-remove");
    k.allow(&[DUNE]);
    let script = format!(
        "tap {}\ntap {}\ntap {}\ntap Dune\ntap {}\nwait\ntap {}",
        EN.sync, EN.link, EN.remove, EN.remove, EN.exit
    );
    let (run, frames) = k.ui(&script, PW11, &[]);
    assert_ok(&run);
    assert!(last_frame_with(&frames, "Sync done: 1 change(s)").contains("Last sync: just now"));
    last_frame_with(&frames, "Remove from StoryGraph?");
    last_frame_with(&frames, "Removed: Dune");
    assert_eq!(k.fake.shelf("dune-0001")["status"], "to-read");
    assert_eq!(k.allowlist().trim(), "");
}

#[test]
fn remove_completely_from_the_kual_entry() {
    let k = Kindle::new("remove-completely");
    k.allow(&[DUNE, SCHWARM]);
    assert_eq!(k.sync().code, 0);
    let script = format!(
        "tap Dune\ntap {}\ntap {}\nwait\ntap {}",
        EN.remove_completely, EN.delete, EN.exit
    );
    let (run, frames) = k.ui(&script, PW11, &["--remove"]);
    assert_ok(&run);
    assert!(
        frames[0].contains("Remove from StoryGraph"),
        "{}",
        frames[0]
    );
    last_frame_with(&frames, "Remove COMPLETELY?");
    last_frame_with(&frames, "Deleted: Dune");
    assert!(k.fake.shelf("dune-0001").is_null());
    assert_eq!(
        k.fake.events_for("dune-0001").last().unwrap(),
        &json!({"book": "dune-0001", "removed": true, "remove_tags": true})
    );
    // The other synced book is untouched.
    assert_eq!(k.fake.shelf("schwarm-0001")["status"], "currently-reading");
}

#[test]
fn toggling_a_book_updates_the_allowlist() {
    let k = Kindle::new("toggle");
    let (run, frames) = k.ui("tap Project Hail Mary\ntap Dune\ntap Dune", PW11, &[]);
    assert_ok(&run);
    assert_eq!(k.allowlist().trim(), "B08FHBV4ZX");
    assert!(frames[1].contains("Synced: 1 / 7"), "{}", frames[1]);
    assert!(frames[3].contains("Synced: 1 / 7"), "{}", frames[3]);
    assert!(k.fake.posts().is_empty());
}

#[test]
fn expired_session_shows_what_to_do() {
    let k = Kindle::new("ui-expired");
    k.allow(&[DUNE]);
    assert_eq!(k.sync().code, 0);
    k.read_to(DUNE, 60.0);
    k.fake.fault(json!({"path": "", "kind": "expired"}));

    let script = format!(
        "tap {}\ntap {}\ntap Dune\nwait\ntap {}\ntap Dune\ntap {}\nwait\ntap {}\ntap {}\ntap {}",
        EN.sync, EN.link, EN.remove, EN.remove, EN.back, EN.back, EN.exit
    );
    let (run, frames) = k.ui(&script, PW11, &[]);
    assert_ok(&run);
    // Sync now, link search and remove all name the cause.
    let expired: Vec<_> = frames
        .iter()
        .filter(|f| f.contains("Session expired - refresh cookies"))
        .collect();
    assert!(expired.len() >= 3, "{}", frames.join("\n"));
    assert!(!frames.iter().any(|f| f.contains("failed (log)")));
    // Nothing changed anywhere.
    assert_eq!(k.fake.shelf("dune-0001")["percent"], 42);
    assert!(k.allowlist().contains(DUNE));
    assert!(!k.state()["books"][DUNE].is_null());
}

#[test]
fn sync_errors_are_named_not_hidden() {
    let k = Kindle::new("ui-errors");
    k.allow(&[DUNE]);

    // Another pass holds the state lock: the UI used to show just
    // "sync failed".
    let lock = k.dir.join("state.json.lock");
    let mut holder = Command::new("python3")
        .args([
            "-I",
            "-c",
            "import fcntl, sys, time\n\
             f = open(sys.argv[1], 'w')\n\
             fcntl.flock(f, fcntl.LOCK_EX)\n\
             print('locked', flush=True)\n\
             time.sleep(30)",
        ])
        .arg(&lock)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(holder.stdout.as_mut().unwrap()),
        &mut line,
    )
    .unwrap();
    let (run, frames) = k.ui(&format!("tap {}", EN.sync), PW11, &[]);
    let _ = holder.kill();
    let _ = holder.wait();
    assert_ok(&run);
    last_frame_with(&frames, "Sync already running - retry later");
    assert!(k.fake.posts().is_empty());

    // StoryGraph answers 500 to the search.
    k.fake.fault(json!({"path": "/browse", "kind": "500"}));
    let (run, frames) = k.ui(&format!("tap {}\ntap Dune\nwait", EN.link), PW11, &[]);
    assert_ok(&run);
    last_frame_with(&frames, "StoryGraph error - try again later");

    // No cookies at all.
    k.set_config("session_cookie", "");
    let (run, frames) = k.ui(&format!("tap {}", EN.sync), PW11, &[]);
    assert_ok(&run);
    assert!(frames[0].contains("Session: not set"));
    last_frame_with(&frames, "No cookies - run setup first");
}

#[test]
fn cancel_a_slow_search() {
    let k = Kindle::new("ui-cancel");
    k.fake
        .fault(json!({"path": "/browse", "kind": "slow", "delay": 3}));
    let script = format!(
        "tap {}\ntap Dune\ntap {}\ntap {}",
        EN.link, EN.cancel, EN.back
    );
    let (run, frames) = k.ui(&script, PW11, &[]);
    assert_ok(&run);
    assert!(frames[2].contains("Searching..."), "{}", frames[2]);
    assert!(frames[3].contains("Link a book"), "{}", frames[3]);
    assert!(frames[4].contains("Not synced yet"), "{}", frames[4]);
    assert!(std::fs::metadata(k.dir.join("manual_mappings.ini")).is_err());
}

#[test]
fn paging_through_a_long_list_on_a_small_screen() {
    let k = Kindle::new("ui-paging");
    let db = rusqlite::Connection::open(k.dir.join("cc.db")).unwrap();
    for i in 0..12 {
        db.execute(
            "INSERT INTO Entries VALUES (?1, 'Entry:Item', 0, ?2, 'EBOK', ?3, '[]', 20.0, ?4)",
            rusqlite::params![
                format!("u-extra-{i}"),
                format!("B0EXTRA{i:03}"),
                format!("Extra Book {i:02}"),
                1_750_000_000 - i
            ],
        )
        .unwrap();
    }
    drop(db);

    let (run, frames) = k.ui("tap v 10 more\ntap v 1 more\ntap ^ BOOKS", KT, &[]);
    assert_ok(&run);
    assert!(frames[0].contains("BOOKS (1-9/19)"), "{}", frames[0]);
    assert!(frames[1].contains("^ BOOKS (10-18/19)"), "{}", frames[1]);
    assert!(frames[2].contains("^ BOOKS (11-19/19)"), "{}", frames[2]);
    assert!(frames[3].contains("BOOKS (2-10/19)"), "{}", frames[3]);
}

#[test]
fn load_error_is_shown_and_closed_by_a_tap() {
    let k = Kindle::new("ui-load-error");
    std::fs::remove_file(k.dir.join("cc.db")).unwrap();
    let (run, frames) = k.ui("tap 10 10", PW11, &[]);
    assert_ok(&run);
    assert_eq!(frames.len(), 1, "{}", frames.join("\n"));
    assert!(frames[0].contains("Error:"), "{}", frames[0]);
    assert!(frames[0].contains("opening cc.db"), "{}", frames[0]);
    assert!(frames[0].contains("Tap to close"), "{}", frames[0]);
}

#[test]
fn script_mistakes_fail_cleanly() {
    let k = Kindle::new("ui-script");
    let (run, _) = k.ui("tap [ Nonexistent ]", PW11, &[]);
    run.assert_clean_error();
    assert!(
        run.stderr.contains("is not on the screen"),
        "{}",
        run.stderr
    );

    let (run, _) = k.ui("jump", PW11, &[]);
    run.assert_clean_error();
    assert!(run.stderr.contains("script line 1"), "{}", run.stderr);

    let (run, _) = k.ui("", "5x5", &[]);
    run.assert_clean_error();
    assert!(run.stderr.contains("too small"), "{}", run.stderr);
}
