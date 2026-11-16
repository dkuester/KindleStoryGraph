//! Shared harness for the end-to-end tests (`tests/e2e.rs`, `tests/ui.rs`):
//! the fake StoryGraph (`tests/e2e/fake_storygraph.py`) and a "virtual
//! Kindle" on the host (temp config, fixture cc.db, the real binary).
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use serde_json::{Value, json};

pub const BIN: &str = env!("CARGO_BIN_EXE_kindlestorygraph");
pub const ROOT: &str = env!("CARGO_MANIFEST_DIR");

// Fixture cc.db keys (tests/emu/cc_fixture.sql).
pub const DUNE: &str = "B00DUNE001"; // 42.3 %
pub const PHM: &str = "B08FHBV4ZX"; // 100 % (finished)
pub const OUTSIDER_KING: &str = "B07D7CWN1L"; // 30 %
pub const OUTSIDER_CAMUS: &str = "B0058ZGGFE"; // 55 %
pub const SCHWARM: &str = "B004OL2OSI"; // 12.5 %
pub const PDOC: &str = "PDOC7XKQ2M9VBN4T"; // 15 %, not on StoryGraph

// ── fake server ──────────────────────────────────────────────────────────────

pub struct Fake {
    child: Child,
    pub base: String,
    http: reqwest::blocking::Client,
}

impl Fake {
    pub fn start() -> Self {
        let mut child = Command::new("python3")
            .arg("-I")
            .arg(Path::new(ROOT).join("tests/e2e/fake_storygraph.py"))
            .args(["--host", "127.0.0.1", "--port", "0"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("start python3 tests/e2e/fake_storygraph.py");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let base = line
            .trim()
            .strip_prefix("listening on ")
            .unwrap_or_else(|| panic!("unexpected fake server banner {line:?}"))
            .to_owned();
        Fake {
            child,
            base,
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn control(&self, path: &str, body: Value) {
        let res = self
            .http
            .post(format!("{}{path}", self.base))
            .body(body.to_string())
            .send()
            .unwrap();
        assert!(res.status().is_success(), "{path}: {}", res.status());
    }

    pub fn reset(&self, spec: Value) {
        self.control("/__reset", spec);
    }

    pub fn fault(&self, fault: Value) {
        self.control("/__fault", fault);
    }

    pub fn clear_faults(&self) {
        self.control("/__clear_faults", json!({}));
    }

    pub fn state(&self) -> Value {
        self.http
            .get(format!("{}/__state", self.base))
            .send()
            .unwrap()
            .json()
            .unwrap()
    }

    pub fn shelf(&self, id: &str) -> Value {
        self.state()["shelf"][id].clone()
    }

    pub fn posts(&self) -> Vec<String> {
        self.state()["requests"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["method"] == "POST")
            .map(|r| r["path"].as_str().unwrap().to_owned())
            .collect()
    }

    pub fn events_for(&self, id: &str) -> Vec<Value> {
        self.state()["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["book"] == id)
            .cloned()
            .collect()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ── virtual Kindle on the host ───────────────────────────────────────────────

pub struct Kindle {
    pub dir: PathBuf,
    pub fake: Fake,
}

pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", self.stdout))
    }

    /// The outcome of one book in `sync` output.
    pub fn outcome(&self, cde_key: &str) -> Value {
        self.json()
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["cde_key"] == cde_key)
            .unwrap_or_else(|| panic!("no outcome for {cde_key}: {}", self.stdout))
            .clone()
    }

    pub fn assert_clean_error(&self) {
        assert_ne!(self.code, 0, "expected failure:\n{}", self.stdout);
        assert!(
            !self.stderr.contains("unexpected error"),
            "error reported as a bug:\n{}",
            self.stderr
        );
    }
}

impl Kindle {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ksg-e2e-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let db = rusqlite::Connection::open(dir.join("cc.db")).unwrap();
        db.execute_batch(
            &std::fs::read_to_string(Path::new(ROOT).join("tests/emu/cc_fixture.sql")).unwrap(),
        )
        .unwrap();

        let p = |f: &str| dir.join(f).display().to_string();
        let config = format!(
            "session_cookie=fake-session-cookie\n\
             remember_user_token=fake-remember-token\n\
             user_slug=\n\
             cc_db_path={}\nstate_path={}\nmanual_mappings_path={}\n\
             allowlist_path={}\nsearch_cache_path={}\nmenu_path={}\n\
             finish_threshold=99.5\nprogress_step=1.0\nmatch_threshold=0.6\n\
             min_progress=2.0\nsync_scope=allowlist\n",
            p("cc.db"),
            p("state.json"),
            p("manual_mappings.ini"),
            p("allowlist.ini"),
            p("search_cache.json"),
            p("menu.json"),
        );
        std::fs::write(dir.join("config.ini"), config).unwrap();

        Kindle {
            dir,
            fake: Fake::start(),
        }
    }

    pub fn allow(&self, keys: &[&str]) {
        std::fs::write(self.dir.join("allowlist.ini"), keys.join("\n") + "\n").unwrap();
    }

    pub fn allowlist(&self) -> String {
        std::fs::read_to_string(self.dir.join("allowlist.ini")).unwrap_or_default()
    }

    pub fn state(&self) -> Value {
        std::fs::read_to_string(self.dir.join("state.json"))
            .map(|s| serde_json::from_str(&s).unwrap())
            .unwrap_or(Value::Null)
    }

    /// Sets a book's progress on the "Kindle" (cc.db).
    pub fn read_to(&self, cde_key: &str, percent: f64) {
        let db = rusqlite::Connection::open(self.dir.join("cc.db")).unwrap();
        let n = db
            .execute(
                "UPDATE Entries SET p_percentFinished = ?1 WHERE p_cdeKey = ?2 AND p_isArchived = 0",
                rusqlite::params![percent, cde_key],
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.args(args)
            .env("KINDLE_STORYGRAPH_CONFIG", self.dir.join("config.ini"))
            .env("KSG_STORYGRAPH_BASE_URL", &self.fake.base)
            .env("KSG_STORYGRAPH_SEARCH_TIMEOUT_MS", "500");
        cmd
    }

    pub fn run(&self, args: &[&str]) -> Run {
        to_run(self.command(args).output().unwrap())
    }

    pub fn run_stdin(&self, args: &[&str], stdin: &str) -> Run {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        to_run(child.wait_with_output().unwrap())
    }

    pub fn sync(&self) -> Run {
        self.run(&["sync"])
    }

    /// Sets (or adds) a key in the temp config.ini.
    pub fn set_config(&self, key: &str, value: &str) {
        let path = self.dir.join("config.ini");
        let old = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = old
            .lines()
            .filter(|l| !l.starts_with(&format!("{key}=")))
            .map(str::to_owned)
            .collect();
        lines.push(format!("{key}={value}"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    }

    /// Runs the UI headless with `script` on a `size` (COLSxROWS) screen;
    /// returns the run and the frames in order.
    pub fn ui(&self, script: &str, size: &str, extra: &[&str]) -> (Run, Vec<String>) {
        let script_path = self.dir.join("ui-script.txt");
        std::fs::write(&script_path, script).unwrap();
        let frames = self.dir.join("frames");
        let _ = std::fs::remove_dir_all(&frames);
        let mut args = vec![
            "ui".to_owned(),
            "--script".to_owned(),
            script_path.display().to_string(),
            "--frames".to_owned(),
            frames.display().to_string(),
            "--size".to_owned(),
            size.to_owned(),
        ];
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let run = self.run(&args);
        let mut names: Vec<_> = std::fs::read_dir(&frames)
            .map(|d| d.map(|e| e.unwrap().path()).collect())
            .unwrap_or_default();
        names.sort();
        let frames = names
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        (run, frames)
    }
}

impl Drop for Kindle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn to_run(out: Output) -> Run {
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}
