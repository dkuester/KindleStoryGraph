use std::path::PathBuf;

use argh::FromArgs;

use kindlestorygraph::config::CONFIG;
use kindlestorygraph::daemon;
use kindlestorygraph::error::UserError;
use kindlestorygraph::reader::{self, Reader};
use kindlestorygraph::storygraph::{self, parse_status_arg};

/// storygraph sync for the native Kindle reader.
#[derive(FromArgs, PartialEq, Debug)]
struct Args {
    #[argh(subcommand)]
    command: Command,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Command {
    Snapshot(Snapshot),
    Sync(Sync),
    Daemon(Daemon),
    Login(Login),
    Search(Search),
    Whoami(Whoami),
    UserBook(UserBook),
    Status(Status),
    Progress(Progress),
}

/// read the Kindle content catalog (cc.db) and print the reading snapshot.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "snapshot")]
struct Snapshot {
    /// path to cc.db
    #[argh(positional)]
    path: String,

    /// percent at or above which a book counts as finished (default 99.5)
    #[argh(option)]
    threshold: Option<f64>,
}

/// run one sync pass: read cc.db, diff against state, push changes to StoryGraph.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "sync")]
struct Sync {
    /// path to cc.db (default: config cc_db_path)
    #[argh(option)]
    cc_db: Option<String>,

    /// path to the state file (default: config state_path)
    #[argh(option)]
    state: Option<String>,

    /// dry run: match and plan only, write nothing to StoryGraph or state
    #[argh(switch)]
    dry_run: bool,
}

/// run the sync daemon in a loop.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "daemon")]
struct Daemon {
    /// poll interval in seconds (default: config poll_interval)
    #[argh(option)]
    interval: Option<u64>,
}

/// log in to StoryGraph; the password is read from stdin.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "login")]
struct Login {
    /// storygraph account email
    #[argh(positional)]
    email: String,
}

/// search StoryGraph for a book.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "search")]
struct Search {
    /// title and/or author query
    #[argh(positional)]
    query: String,
}

/// print the logged-in StoryGraph user.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "whoami")]
struct Whoami {}

/// fetch the current user's status and progress for a StoryGraph book.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "userbook")]
struct UserBook {
    /// storygraph book id
    #[argh(positional)]
    book_id: String,
}

/// set the reading status of a StoryGraph book.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "status")]
struct Status {
    /// storygraph book id
    #[argh(positional)]
    book_id: String,

    /// to-read | currently-reading | read | paused | did-not-finish | rereading
    #[argh(positional)]
    status: String,
}

/// update reading progress (percentage) of a StoryGraph book.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "progress")]
struct Progress {
    /// storygraph book id
    #[argh(positional)]
    book_id: String,

    /// progress percentage 0-100
    #[argh(positional)]
    percent: u32,
}

fn main() {
    let args: Args = argh::from_env();
    if let Err(e) = run(args.command) {
        report_error(e);
        std::process::exit(1);
    }
}

fn run(cmd: Command) -> anyhow::Result<()> {
    match cmd {
        Command::Snapshot(s) => {
            let threshold = s.threshold.unwrap_or(reader::DEFAULT_FINISH_THRESHOLD);
            let r = Reader::open_read_only(&s.path)?;
            let snapshot = r.snapshot(threshold)?;
            let current = snapshot
                .iter()
                .find(|b| b.status == reader::Status::CurrentlyReading);
            print_json(&serde_json::json!({
                "finish_threshold": threshold,
                "current": current,
                "in_progress": snapshot
                    .iter()
                    .filter(|b| b.status == reader::Status::CurrentlyReading)
                    .collect::<Vec<_>>(),
                "finished": snapshot
                    .iter()
                    .filter(|b| b.status == reader::Status::Read)
                    .collect::<Vec<_>>(),
            }));
        }
        Command::Sync(s) => {
            if s.dry_run {
                eprintln!("DRY RUN — nothing is written to StoryGraph or the state file.");
            }
            let cc_db = PathBuf::from(s.cc_db.unwrap_or_else(|| CONFIG.cc_db_path.clone()));
            let state = PathBuf::from(s.state.unwrap_or_else(|| CONFIG.state_path.clone()));
            let outcomes = daemon::sync_once_from_paths(&cc_db, &state, s.dry_run)?;
            print_json(&outcomes);
        }
        Command::Daemon(d) => {
            let interval = d.interval.unwrap_or(CONFIG.poll_interval);
            daemon::run(interval)?;
        }
        Command::Login(l) => {
            let mut password = String::new();
            std::io::stdin().read_line(&mut password)?;
            let result = storygraph::login(&l.email, password.trim_end())?;
            print_json(&result);
            eprintln!(
                "Add these to config.ini: session_cookie=<value> and remember_user_token=<value>"
            );
        }
        Command::Search(s) => print_json(&storygraph::search(&s.query)?),
        Command::Whoami(_) => print_json(&storygraph::get_user()?),
        Command::UserBook(u) => print_json(&storygraph::get_user_book(&u.book_id)?),
        Command::Status(st) => {
            let status_id = parse_status_arg(&st.status)
                .map(|r| r as u8)
                .ok_or_else(|| UserError(format!("Unknown status '{}'", st.status)))?;
            print_json(&storygraph::update_status(&st.book_id, status_id)?);
        }
        Command::Progress(p) => {
            print_json(&storygraph::update_progress(
                &p.book_id,
                "percentage",
                p.percent,
            )?);
        }
    }
    Ok(())
}

fn print_json<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).expect("serialize"));
}

fn report_error(e: anyhow::Error) {
    if let Some(u) = e.downcast_ref::<UserError>() {
        eprintln!("{}", u.0);
    } else {
        eprintln!("Encountered an unexpected error. Please report this.\n\n{e:#}");
    }
}
