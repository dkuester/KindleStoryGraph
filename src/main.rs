use std::path::{Path, PathBuf};

use argh::FromArgs;

use kindlestorygraph::config::{self, CONFIG};
use kindlestorygraph::daemon;
use kindlestorygraph::error::UserError;
use kindlestorygraph::menu;
use kindlestorygraph::reader::{self, Reader};
use kindlestorygraph::setup;
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
    Books(Books),
    Toggle(Toggle),
    Menu(Menu),
    FindMatches(FindMatches),
    Link(Link),
    Doctor(Doctor),
    ImportCookies(ImportCookies),
    Ui(Ui),
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

    /// minimum percent to include; books below are not shown (default 0)
    #[argh(option)]
    min_progress: Option<f64>,
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

/// list in-progress books and whether each is allowlisted for sync.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "books")]
struct Books {
    /// path to cc.db (default: config cc_db_path)
    #[argh(option)]
    cc_db: Option<String>,
}

/// toggle whether a book (by cde_key) is synced (allowlist).
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "toggle")]
struct Toggle {
    /// the Kindle content key (cde_key) of the book
    #[argh(positional)]
    cde_key: String,
}

/// print the KUAL menu.json (with a per-book allowlist selection).
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "menu")]
struct Menu {
    /// path to cc.db (default: config cc_db_path)
    #[argh(option)]
    cc_db: Option<String>,

    /// write the menu to config menu_path instead of printing it
    #[argh(switch)]
    write: bool,
}

/// search StoryGraph for unlinked books and cache candidates for the pick-list.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "find-matches")]
struct FindMatches {
    /// path to cc.db (default: config cc_db_path)
    #[argh(option)]
    cc_db: Option<String>,

    /// max candidates cached per book (default 5)
    #[argh(option, default = "5")]
    limit: usize,
}

/// link a Kindle book to a StoryGraph book in manual_mappings.ini.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "link")]
struct Link {
    /// the Kindle content key (cde_key)
    #[argh(positional)]
    cde_key: String,

    /// the StoryGraph book id
    #[argh(positional)]
    storygraph_id: String,
}

/// print a health report: config, cc.db, allowlist, state, optional network.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "doctor")]
struct Doctor {
    /// path to cc.db (default: config cc_db_path)
    #[argh(option)]
    cc_db: Option<String>,

    /// also verify the StoryGraph session over the network
    #[argh(switch)]
    network: bool,
}

/// import StoryGraph session cookies from a Netscape-format cookies.txt.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "import-cookies")]
struct ImportCookies {
    /// path to the exported cookies.txt
    #[argh(positional)]
    path: String,
}

/// open the on-device settings UI (drawn directly to the e-ink framebuffer).
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "ui")]
struct Ui {}

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
            let min_progress = s.min_progress.unwrap_or(0.0);
            let r = Reader::open_read_only(&s.path)?;
            let snapshot = r.snapshot(threshold, min_progress)?;
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
        Command::Books(b) => {
            let cc_db = PathBuf::from(b.cc_db.unwrap_or_else(|| CONFIG.cc_db_path.clone()));
            let reader = Reader::open_read_only(cc_db.to_str().unwrap_or_default())?;
            let snapshot = reader.snapshot(CONFIG.finish_threshold, CONFIG.min_progress)?;
            let allowlist = config::load_allowlist(Path::new(&CONFIG.allowlist_path));
            println!("ALLOWED\tCDE_KEY\tPERCENT\tTITLE");
            for bk in &snapshot {
                println!(
                    "{}\t{}\t{:.2}\t{}",
                    if allowlist.contains(&bk.cde_key) {
                        "yes"
                    } else {
                        "no"
                    },
                    bk.cde_key,
                    bk.percent,
                    bk.title
                );
            }
        }
        Command::Toggle(t) => {
            let allowed = config::toggle_allowlist(Path::new(&CONFIG.allowlist_path), &t.cde_key)?;
            println!("{}", if allowed { "allowed" } else { "skipped" });
            // Regenerate the menu in the same process: KUAL Next re-reads
            // menu.json ~250 ms after starting the action.
            if let Err(e) = menu::write_menu(Path::new(&CONFIG.cc_db_path)) {
                eprintln!("warning: could not update the KUAL menu: {e:#}");
            }
        }
        Command::Menu(m) => {
            let cc_db = PathBuf::from(m.cc_db.unwrap_or_else(|| CONFIG.cc_db_path.clone()));
            if m.write {
                menu::write_menu(&cc_db)?;
                println!("menu written to {}", CONFIG.menu_path);
            } else {
                print_json(&menu::build_menu(&cc_db));
            }
        }
        Command::FindMatches(f) => {
            let cc_db = PathBuf::from(f.cc_db.unwrap_or_else(|| CONFIG.cc_db_path.clone()));
            let searched = menu::find_matches(&cc_db, f.limit)?;
            println!("Searched {searched} book(s); candidates cached for 'Link books'.");
        }
        Command::Link(l) => {
            config::add_manual_mapping(
                Path::new(&CONFIG.manual_mappings_path),
                &l.cde_key,
                &l.storygraph_id,
            )?;
            println!("Linked {} -> {}", l.cde_key, l.storygraph_id);
            if let Err(e) = menu::write_menu(Path::new(&CONFIG.cc_db_path)) {
                eprintln!("warning: could not update the KUAL menu: {e:#}");
            }
        }
        Command::Doctor(d) => {
            let cc_db = PathBuf::from(d.cc_db.unwrap_or_else(|| CONFIG.cc_db_path.clone()));
            setup::doctor(&cc_db, d.network)?;
        }
        Command::ImportCookies(i) => setup::import_cookies(Path::new(&i.path))?,
        Command::Ui(_) => kindlestorygraph::ui::run()?,
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
