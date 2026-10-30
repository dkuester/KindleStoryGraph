//! End-to-end tests (stage 2 of the virtual Kindle, docs/emulator.md): the
//! real host-built binary against the fake StoryGraph in
//! `tests/e2e/fake_storygraph.py`, with a temp config, a fixture cc.db built
//! from `tests/emu/cc_fixture.sql` and fake cookies.
//!
//! Every test starts its own fake server (port 0) and its own temp dir, so the
//! tests run in parallel. Each asserts on both sides: the CLI outcome (JSON,
//! exit code, messages) and the fake server's state (shelf, events, requests).
//! Requires `python3` on the PATH.

mod common;

use std::time::{Duration, Instant};

use common::*;
use serde_json::json;

// ── tests ────────────────────────────────────────────────────────────────────

#[test]
fn first_sync_attaches_and_sets_status_and_progress() {
    let k = Kindle::new("first");
    k.allow(&[DUNE, SCHWARM, PDOC]);

    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    // Dune: the exact digital edition, not "Dune Messiah" (listed first) nor
    // the audio edition.
    assert_eq!(
        r.outcome(DUNE),
        json!({"action": "attached", "cde_key": DUNE, "storygraph_id": "dune-0001", "percent": 42})
    );
    assert_eq!(r.outcome(SCHWARM)["storygraph_id"], "schwarm-0001");
    assert_eq!(r.outcome(SCHWARM)["percent"], 13);
    assert_eq!(r.outcome(PDOC)["action"], "no_match");

    assert_eq!(
        k.fake.shelf("dune-0001"),
        json!({"status": "currently-reading", "percent": 42})
    );
    assert_eq!(
        k.fake.events_for("dune-0001"),
        vec![
            json!({"book": "dune-0001", "status": "currently-reading"}),
            json!({"book": "dune-0001", "progress": 42}),
        ]
    );
    assert_eq!(
        k.fake.shelf("schwarm-0001"),
        json!({"status": "currently-reading", "percent": 13})
    );
    let shelf = k.fake.state()["shelf"].clone();
    assert_eq!(shelf.as_object().unwrap().len(), 2, "{shelf}");

    let st = k.state();
    assert_eq!(st["books"][DUNE]["storygraph_id"], "dune-0001");
    assert_eq!(st["books"][DUNE]["status"], "currently-reading");
    assert!(st["books"][PDOC].is_null());
}

#[test]
fn progress_update_and_unchanged_pass() {
    let k = Kindle::new("progress");
    k.allow(&[DUNE]);
    assert_eq!(k.sync().code, 0);

    k.read_to(DUNE, 50.4);
    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.outcome(DUNE),
        json!({"action": "progress_updated", "cde_key": DUNE, "percent": 50})
    );
    assert_eq!(
        k.fake.shelf("dune-0001"),
        json!({"status": "currently-reading", "percent": 50})
    );
    assert_eq!(k.state()["books"][DUNE]["last_percent"], 50.4);

    // Below progress_step: no request that writes anything.
    let posts_before = k.fake.posts().len();
    k.read_to(DUNE, 50.9);
    let r = k.sync();
    assert_eq!(r.outcome(DUNE)["action"], "unchanged");
    assert_eq!(k.fake.posts().len(), posts_before);
}

#[test]
fn backward_progress_guard_skips_when_remote_is_ahead() {
    let k = Kindle::new("guard");
    k.fake.reset(json!({"shelf": {
        "dune-0001": {"status": "currently-reading", "percent": 60},
        "schwarm-0001": {"status": "read", "percent": 100},
    }}));
    k.allow(&[DUNE, SCHWARM]);

    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.outcome(DUNE),
        json!({"action": "skipped_behind", "cde_key": DUNE, "storygraph_id": "dune-0001",
               "percent": 42, "remote_percent": 60, "remote_read": false})
    );
    let schwarm = r.outcome(SCHWARM);
    assert_eq!(schwarm["action"], "skipped_behind");
    assert_eq!(schwarm["remote_read"], true);

    // Nothing written; the read book is still read (no "rereading").
    assert!(k.fake.posts().is_empty(), "{:?}", k.fake.posts());
    assert_eq!(
        k.fake.shelf("dune-0001"),
        json!({"status": "currently-reading", "percent": 60})
    );
    assert_eq!(
        k.fake.shelf("schwarm-0001"),
        json!({"status": "read", "percent": 100})
    );
    // Linked in state, mirroring the remote status.
    assert_eq!(k.state()["books"][SCHWARM]["status"], "read");

    // Reading further on the Kindle: still behind StoryGraph's read book.
    k.read_to(SCHWARM, 40.0);
    let r = k.sync();
    assert_eq!(r.outcome(SCHWARM)["action"], "skipped_behind");
    assert_eq!(r.outcome(SCHWARM)["remote_read"], true);
    assert!(k.fake.posts().is_empty());
    assert_eq!(k.fake.shelf("schwarm-0001")["status"], "read");
}

#[test]
fn finishing_marks_read_without_rereading() {
    let k = Kindle::new("finish");
    k.allow(&[DUNE, PHM]);

    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    // A book first seen finished: status "read" only, no progress push.
    assert_eq!(r.outcome(PHM)["action"], "attached");
    assert_eq!(
        k.fake.events_for("phm-0001"),
        vec![json!({"book": "phm-0001", "status": "read"})]
    );

    k.read_to(DUNE, 100.0);
    let r = k.sync();
    assert_eq!(
        r.outcome(DUNE),
        json!({"action": "finished", "cde_key": DUNE})
    );
    assert_eq!(
        k.fake.shelf("dune-0001"),
        json!({"status": "read", "percent": 100})
    );
    let events = k.fake.events_for("dune-0001");
    assert_eq!(
        events.last().unwrap(),
        &json!({"book": "dune-0001", "status": "read"})
    );
    assert!(
        !events.iter().any(|e| e["status"] == "rereading"),
        "{events:?}"
    );
    assert_eq!(k.state()["books"][DUNE]["status"], "read");

    // The next pass leaves the finished book alone.
    let posts_before = k.fake.posts().len();
    assert_eq!(k.sync().outcome(DUNE)["action"], "unchanged");
    assert_eq!(k.fake.posts().len(), posts_before);
    assert_eq!(k.fake.shelf("dune-0001")["status"], "read");
}

#[test]
fn author_aware_match_picks_the_right_edition() {
    let k = Kindle::new("author");
    k.allow(&[OUTSIDER_KING, OUTSIDER_CAMUS]);

    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.outcome(OUTSIDER_KING)["storygraph_id"],
        "outsider-king-0001"
    );
    assert_eq!(
        r.outcome(OUTSIDER_CAMUS)["storygraph_id"],
        "outsider-camus-0001"
    );
    assert_eq!(k.fake.shelf("outsider-king-0001")["percent"], 30);
    assert_eq!(k.fake.shelf("outsider-camus-0001")["percent"], 55);
}

#[test]
fn one_failing_book_does_not_stop_the_others() {
    let k = Kindle::new("isolation");
    k.allow(&[DUNE, OUTSIDER_KING, SCHWARM]);
    k.fake
        .fault(json!({"path": "/books/outsider-king-0001", "kind": "500"}));

    let r = k.sync();
    r.assert_clean_error();
    assert!(r.stderr.contains("1 book(s) failed"), "{}", r.stderr);
    let failed = r.outcome(OUTSIDER_KING);
    assert_eq!(failed["action"], "failed");
    assert!(
        failed["error"].as_str().unwrap().contains("500"),
        "{failed}"
    );
    assert_eq!(r.outcome(DUNE)["action"], "attached");
    assert_eq!(r.outcome(SCHWARM)["action"], "attached");

    assert!(k.fake.shelf("outsider-king-0001").is_null());
    assert_eq!(k.fake.shelf("dune-0001")["percent"], 42);
    assert_eq!(k.fake.shelf("schwarm-0001")["percent"], 13);
    // The others' state was saved; the failed book is retried next pass.
    let st = k.state();
    assert!(st["books"][OUTSIDER_KING].is_null());
    assert_eq!(st["books"][SCHWARM]["storygraph_id"], "schwarm-0001");

    k.fake.clear_faults();
    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.outcome(OUTSIDER_KING)["action"], "attached");
    assert_eq!(k.fake.shelf("outsider-king-0001")["percent"], 30);
}

#[test]
fn expired_session_gives_a_clean_message() {
    let k = Kindle::new("expired");
    k.allow(&[DUNE]);
    assert_eq!(k.sync().code, 0);
    k.fake.fault(json!({"path": "", "kind": "expired"}));

    k.read_to(DUNE, 60.0);
    let r = k.sync();
    r.assert_clean_error();
    let failed = r.outcome(DUNE);
    assert_eq!(failed["action"], "failed");
    assert!(
        failed["error"]
            .as_str()
            .unwrap()
            .contains("session has expired"),
        "{failed}"
    );
    assert!(r.stderr.contains("session has expired"), "{}", r.stderr);
    assert_eq!(k.fake.shelf("dune-0001")["percent"], 42);
    assert_eq!(k.state()["books"][DUNE]["last_percent"], 42.3);

    for args in [&["whoami"][..], &["unsync", DUNE], &["search", "Dune"]] {
        let r = k.run(args);
        r.assert_clean_error();
        assert!(
            r.stderr.contains("session has expired"),
            "{args:?}: {}",
            r.stderr
        );
    }
    // A failed unsync leaves allowlist and state untouched.
    assert!(k.allowlist().contains(DUNE));
    assert!(!k.state()["books"][DUNE].is_null());
}

#[test]
fn unsync_resets_to_read_and_completely_removes() {
    let k = Kindle::new("unsync");
    k.fake.reset(json!({"tags": ["schwarm-0001"]}));
    k.allow(&[DUNE, SCHWARM, OUTSIDER_KING]);
    assert_eq!(k.sync().code, 0);

    // Plain unsync: back to "to-read", off the allowlist and out of state.
    let r = k.run(&["unsync", DUNE]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("set to to-read"), "{}", r.stdout);
    assert_eq!(k.fake.shelf("dune-0001")["status"], "to-read");
    assert!(!k.allowlist().contains(DUNE));
    assert!(k.state()["books"][DUNE].is_null());

    // --completely, book without tags: gone, tags removed too.
    let r = k.run(&["unsync", "--completely", OUTSIDER_KING]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(k.fake.shelf("outsider-king-0001").is_null());
    assert_eq!(
        k.fake.events_for("outsider-king-0001").last().unwrap(),
        &json!({"book": "outsider-king-0001", "removed": true, "remove_tags": true})
    );
    assert!(k.state()["books"][OUTSIDER_KING].is_null());

    // --completely, book with tags: gone, the user's tags kept.
    let r = k.run(&["unsync", "--completely", SCHWARM]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(k.fake.shelf("schwarm-0001").is_null());
    assert_eq!(k.fake.state()["tags"], json!(["schwarm-0001"]));
    assert_eq!(
        k.fake.events_for("schwarm-0001").last().unwrap()["remove_tags"],
        false
    );
    assert_eq!(k.allowlist().trim(), "");
    assert_eq!(k.state()["books"], json!({}));

    // The next pass does not put any of them back.
    let posts_before = k.fake.posts().len();
    let r = k.sync();
    assert_eq!(r.code, 0);
    assert_eq!(r.json(), json!([]));
    assert_eq!(k.fake.posts().len(), posts_before);

    // Never synced: refused cleanly, nothing sent.
    let r = k.run(&["unsync", PHM]);
    r.assert_clean_error();
    assert!(r.stderr.contains("has not been synced"), "{}", r.stderr);
    assert_eq!(k.fake.posts().len(), posts_before);
}

#[test]
fn dry_run_writes_nothing() {
    let k = Kindle::new("dryrun");
    k.fake.reset(json!({"shelf": {
        "outsider-camus-0001": {"status": "currently-reading", "percent": 80},
    }}));
    k.allow(&[DUNE, PHM, OUTSIDER_CAMUS, PDOC]);
    let before = k.fake.state()["shelf"].clone();

    let r = k.run(&["sync", "--dry-run"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stderr.contains("DRY RUN"));
    assert_eq!(r.outcome(DUNE)["action"], "attached");
    assert_eq!(r.outcome(PHM)["action"], "attached");
    assert_eq!(r.outcome(OUTSIDER_CAMUS)["action"], "skipped_behind");
    assert_eq!(r.outcome(PDOC)["action"], "no_match");

    assert!(k.fake.posts().is_empty(), "{:?}", k.fake.posts());
    assert_eq!(k.fake.state()["shelf"], before);
    assert_eq!(k.fake.state()["events"], json!([]));
    assert!(!k.dir.join("state.json").exists());
}

#[test]
fn search_timeout_surfaces_cleanly() {
    let k = Kindle::new("timeout");
    k.allow(&[DUNE, SCHWARM]);
    k.fake.fault(json!({
        "path": "/browse", "kind": "slow", "delay": 3, "times": 1000
    }));

    let t0 = Instant::now();
    let r = k.sync();
    let took = t0.elapsed();
    r.assert_clean_error();
    for key in [DUNE, SCHWARM] {
        let o = r.outcome(key);
        assert_eq!(o["action"], "failed");
        assert!(o["error"].as_str().unwrap().contains("timed out"), "{o}");
    }
    assert!(r.stderr.contains("2 book(s) failed"), "{}", r.stderr);
    // 500 ms per attempt, 4 attempts, jittered back-off: far below the
    // production 15 s per attempt.
    assert!(took < Duration::from_secs(30), "took {took:?}");
    assert!(k.fake.posts().is_empty());
    assert_eq!(k.state()["books"], json!({}));
}

#[test]
fn wrong_format_pages_fail_cleanly() {
    let k = Kindle::new("garbage");
    k.allow(&[DUNE, SCHWARM]);
    k.fake
        .fault(json!({"path": "/books/dune-0001", "kind": "garbage"}));
    k.fake
        .fault(json!({"path": "/browse", "kind": "garbage", "times": 1}));

    // A search page without results markup reads as "no match" (the first
    // search; the next one works) — silently, see "Drift" in
    // docs/emulator.md.
    let r = k.sync();
    assert_eq!(r.code, 0, "{}", r.stderr);
    let dune = r.outcome(DUNE);
    assert_eq!(dune["action"], "no_match", "{dune}");
    assert_eq!(r.outcome(SCHWARM)["action"], "attached");
    assert!(k.fake.shelf("dune-0001").is_null());

    // A book page without the CSRF token fails the book, not the pass.
    let r = k.sync();
    r.assert_clean_error();
    let dune = r.outcome(DUNE);
    assert_eq!(dune["action"], "failed");
    assert!(dune["error"].as_str().unwrap().contains("CSRF"), "{dune}");
    assert!(k.fake.shelf("dune-0001").is_null());
}

#[test]
fn whoami_and_login_against_the_fake() {
    let k = Kindle::new("login");
    let r = k.run(&["whoami"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.json()["slug"], "e2e-reader");

    let r = k.run_stdin(
        &["login", "reader@example.test"],
        "correct horse battery staple\n",
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let j = r.json();
    assert!(
        j["session_cookie"]
            .as_str()
            .unwrap()
            .starts_with("fake-session-")
    );
    assert_eq!(j["remember_user_token"], "fake-remember-token");

    let r = k.run_stdin(&["login", "reader@example.test"], "wrong\n");
    assert_ne!(r.code, 0);
    assert!(r.stderr.contains("Login failed"), "{}", r.stderr);
}

#[test]
fn base_url_override_refuses_non_loopback() {
    let k = Kindle::new("guardurl");
    for url in ["https://app.thestorygraph.com", "http://example.com:8080"] {
        let r = to_run(
            k.command(&["whoami"])
                .env("KSG_STORYGRAPH_BASE_URL", url)
                .output()
                .unwrap(),
        );
        r.assert_clean_error();
        assert!(r.stderr.contains("refused"), "{url}: {}", r.stderr);
    }
}

/// The fake really enforces session and CSRF, so the green tests above prove
/// the client sends both.
#[test]
fn fake_enforces_session_and_csrf() {
    let fake = Fake::start();
    let no_redirect = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = no_redirect
        .get(format!("{}/books/dune-0001", fake.base))
        .send()
        .unwrap();
    assert_eq!(res.status(), 302);
    assert_eq!(res.headers()["location"], "/users/sign_in");

    let res = no_redirect
        .post(format!(
            "{}/update-status.js?book_id=dune-0001&status=read",
            fake.base
        ))
        .header("Cookie", "_storygraph_session=fake-session-cookie")
        .form(&[("authenticity_token", "wrong")])
        .send()
        .unwrap();
    assert_eq!(res.status(), 422);
    assert!(fake.state()["shelf"]["dune-0001"].is_null());
}
