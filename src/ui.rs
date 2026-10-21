//! The FBInk framebuffer settings UI (Kindle, ARM Linux only).
//!
//! Draws directly to the e-ink framebuffer via FBInk (statically linked) and
//! reads touch input from `/dev/input/event*`. No X server, no browser, no
//! toolkit. The pure data/layout helpers live outside the `imp` module so they
//! stay unit-testable on the host.

use anyhow::Result;

/// One book line as shown in the list (pure, host-testable).
#[derive(Debug, Clone)]
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
pub(crate) struct BookRow {
    pub cde_key: String,
    pub title: String,
    pub author: String,
    pub percent: f64,
    pub allowlisted: bool,
    pub linked: bool,
    /// Synced by this tool (in `state.json`), i.e. on the StoryGraph shelf.
    pub on_shelf: bool,
}

/// Build the display rows for the snapshot (pure, host-testable). The badges
/// (`[sync]`, `[linked]`) are folded into a single trailing suffix.
/// `on_shelf` are the books in `state.json` (offered under "Remove").
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
pub(crate) fn build_rows(
    snapshot: &[crate::reader::BookProgress],
    allowlist: &std::collections::HashSet<String>,
    linked: &std::collections::HashMap<String, String>,
    on_shelf: &std::collections::HashSet<String>,
) -> Vec<BookRow> {
    snapshot
        .iter()
        .map(|b| BookRow {
            cde_key: b.cde_key.clone(),
            title: b.title.clone(),
            author: b.author.clone(),
            percent: b.percent,
            allowlisted: allowlist.contains(&b.cde_key),
            linked: linked.contains_key(&b.cde_key),
            on_shelf: on_shelf.contains(&b.cde_key),
        })
        .collect()
}

/// Indices of the rows the "Remove from StoryGraph" list offers: the books
/// this tool synced (pure, host-testable).
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
pub(crate) fn shelf_indices(rows: &[BookRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, b)| b.on_shelf)
        .map(|(i, _)| i)
        .collect()
}

/// Format the read-only settings screen into one line per setting (pure,
/// host-testable). Labels are the config keys; the UI pads/truncates each line
/// to the framebuffer width.
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
pub(crate) fn settings_lines(cfg: &crate::config::Config) -> Vec<String> {
    vec![
        format!("min_progress        {:.1} %", cfg.min_progress),
        format!("finish_threshold    {:.1} %", cfg.finish_threshold),
        format!("language            {}", cfg.language),
        format!("sync_scope          {}", cfg.sync_scope),
        format!("cc.db               {}", cfg.cc_db_path),
        format!("state               {}", cfg.state_path),
        format!("manual mappings     {}", cfg.manual_mappings_path),
        format!("allowlist           {}", cfg.allowlist_path),
    ]
}

/// Truncate to at most `max` chars (with a `~` marker when cut).
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let cut = s.chars().take(max.saturating_sub(1)).collect::<String>();
    format!("{cut}~")
}

/// Left-justify (pad with spaces) to exactly `cols` chars.
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
fn pad_right(s: &str, cols: usize) -> String {
    let mut out = truncate_chars(s, cols);
    out.push_str(&" ".repeat(cols.saturating_sub(out.chars().count())));
    out
}

/// Center a string to exactly `cols` chars.
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
fn pad_center(s: &str, cols: usize) -> String {
    let s = truncate_chars(s, cols);
    let len = s.chars().count();
    let left = cols.saturating_sub(len) / 2;
    let mut out = String::with_capacity(cols);
    out.push_str(&" ".repeat(left));
    out.push_str(&s);
    out.push_str(&" ".repeat(cols.saturating_sub(left + len)));
    out
}

#[cfg(not(all(target_os = "linux", target_arch = "arm")))]
pub fn run() -> Result<()> {
    eprintln!("The `ui` command is only available on the Kindle (ARM Linux).");
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "arm"))]
pub fn run() -> Result<()> {
    imp::run()
}

#[cfg(all(target_os = "linux", target_arch = "arm"))]
mod imp {
    use super::{pad_center, pad_right, truncate_chars};
    use std::ffi::CString;
    use std::os::raw::{c_int, c_void};
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    use anyhow::{Context, Result, bail};

    use crate::config::{
        CONFIG, add_manual_mapping, load_allowlist, load_manual_mappings, toggle_allowlist,
    };
    use crate::daemon;
    use crate::fbink as fb;
    use crate::fbink::{FBInkConfig, FBInkState, InputEvent};
    use crate::menu::{self, Candidate};
    use crate::reader::Reader;
    use crate::state::State;
    use crate::storygraph::Book;
    use crate::sync::{RealStoryGraph, UnsyncMode, Unsynced, unsync_book};

    /// Pick a UI string by configured language (German when `language` is "de").
    fn t(en: &'static str, de: &'static str) -> &'static str {
        if CONFIG.language.eq_ignore_ascii_case("de") {
            de
        } else {
            en
        }
    }

    fn de() -> bool {
        CONFIG.language.eq_ignore_ascii_case("de")
    }

    /// Relative time ("just now", "3 min ago", ...), localized by config.
    fn rel_ago(epoch_secs: i64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let delta = now.saturating_sub(epoch_secs);
        if CONFIG.language.eq_ignore_ascii_case("de") {
            if delta < 60 {
                "gerade eben".into()
            } else if delta < 3600 {
                format!("vor {} Min", delta / 60)
            } else if delta < 86400 {
                format!("vor {} Std", delta / 3600)
            } else {
                format!("vor {} Tagen", delta / 86400)
            }
        } else if delta < 60 {
            "just now".into()
        } else if delta < 3600 {
            format!("{} min ago", delta / 60)
        } else if delta < 86400 {
            format!("{} h ago", delta / 3600)
        } else {
            format!("{} d ago", delta / 86400)
        }
    }

    /// Classify a StoryGraph (search/remove) error into a short,
    /// user-actionable line.
    fn classify_network_error(msg: &str) -> String {
        let lower = msg.to_lowercase();
        if lower.contains("timed out") || lower.contains("timeout") {
            t(
                "Timed out - check network",
                "Zeitüberschreitung - Netz prüfen",
            )
            .to_owned()
        } else if (lower.contains("session") && lower.contains("expired"))
            || lower.contains("sign_in")
        {
            t(
                "Session expired - refresh cookies",
                "Sitzung abgelaufen - Cookies erneuern",
            )
            .to_owned()
        } else if lower.contains("session_cookie") {
            t(
                "No cookies - run setup first",
                "Keine Cookies - erst Setup ausführen",
            )
            .to_owned()
        } else if lower.contains("error sending request")
            || lower.contains("dns")
            || lower.contains("connection")
        {
            t("No network connection", "Keine Netzwerkverbindung").to_owned()
        } else {
            // Raw message; the draw path truncates it to fit.
            msg.to_owned()
        }
    }

    const FONT_MULT: u8 = 3;
    const TITLE_ROW: i16 = 0;
    const STATUS_ROW_1: i16 = 2;
    const STATUS_ROW_2: i16 = 3;
    const STATUS_ROW_3: i16 = 4;
    const SECTION_ROW: i16 = 6;
    const BOOK_START_ROW: i16 = 7;
    const BUTTON_OFFSET_FROM_BOTTOM: i16 = 3;
    const UTIL_OFFSET_FROM_BOTTOM: i16 = 2;
    const HINT_OFFSET_FROM_BOTTOM: i16 = 1;

    /// Owns the framebuffer fd and a zeroed-but-init'd FBInk config.
    struct Fb {
        fbfd: c_int,
        cfg: FBInkConfig,
        fw: usize, // effective cell width (px)
        fh: usize, // effective cell height (px)
        cols: usize,
        rows: usize,
        // Touch correction (from FBInkState device quirks).
        view_vert_origin: usize,
        screen_w: usize,
        screen_h: usize,
        touch_swap_axes: bool,
        touch_mirror_x: bool,
        touch_mirror_y: bool,
    }

    impl Fb {
        fn init() -> Result<Self> {
            let fbfd = unsafe { fb::fbink_open() };
            if fbfd < 0 {
                bail!("could not open the framebuffer (fbink_open failed)");
            }
            let mut cfg = FBInkConfig::default();
            cfg.fontmult = FONT_MULT;
            cfg.fontname = fb::FONT_TERMINUS;
            cfg.is_quiet = true;
            if unsafe { fb::fbink_init(fbfd, &cfg) } < 0 {
                unsafe { fb::fbink_close(fbfd) };
                bail!("fbink_init failed");
            }
            let mut st: FBInkState = unsafe { std::mem::zeroed() };
            unsafe { fb::fbink_get_state(&cfg, &mut st) };
            Ok(Fb {
                fbfd,
                cfg,
                fw: st.font_w as usize,
                fh: st.font_h as usize,
                cols: st.max_cols as usize,
                rows: st.max_rows as usize,
                view_vert_origin: st.view_vert_origin as usize,
                screen_w: st.screen_width as usize,
                screen_h: st.screen_height as usize,
                touch_swap_axes: st.touch_swap_axes,
                touch_mirror_x: st.touch_mirror_x,
                touch_mirror_y: st.touch_mirror_y,
            })
        }

        /// Map a raw panel coordinate to a framebuffer coordinate, honoring the
        /// device's touch quirks (swap first, then mirror each axis).
        fn map_touch(&self, x: i32, y: i32) -> (i32, i32) {
            let (mut x, mut y) = (x, y);
            if self.touch_swap_axes {
                std::mem::swap(&mut x, &mut y);
            }
            if self.touch_mirror_x {
                x = self.screen_w as i32 - 1 - x;
            }
            if self.touch_mirror_y {
                y = self.screen_h as i32 - 1 - y;
            }
            (x, y)
        }

        /// Draw text at a grid cell (batched; no refresh).
        fn print(&self, row: i16, col: i16, text: &str, inverted: bool, centered: bool) {
            let Ok(c) = CString::new(text) else {
                return;
            };
            let mut p = self.cfg;
            p.row = row;
            p.col = col;
            p.is_inverted = inverted;
            p.is_centered = centered;
            p.no_refresh = true;
            unsafe { fb::fbink_print(self.fbfd, c.as_ptr(), &p) };
        }

        /// Clear to background (batched; no refresh).
        fn cls(&self) {
            let mut p = self.cfg;
            p.no_refresh = true;
            unsafe { fb::fbink_cls(self.fbfd, &p, std::ptr::null(), false) };
        }

        /// Flush the batched drawing to the e-ink screen.
        fn refresh(&self, flash: bool) {
            let mut r = self.cfg;
            r.is_flashing = flash;
            r.wfm_mode = fb::WFM_GC16;
            unsafe { fb::fbink_refresh(self.fbfd, 0, 0, 0, 0, &r) };
        }
    }

    impl Drop for Fb {
        fn drop(&mut self) {
            unsafe { fb::fbink_close(self.fbfd) };
        }
    }

    /// Result of a tap: either keep running or leave the UI.
    enum Tap {
        None,
        Redraw,
        Exit,
    }

    /// Which screen the UI is showing.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Screen {
        Main,
        Settings,
        /// Pick a book to (re)link (scrollable list of all books).
        Link,
        /// Search running in the background; the tap loop polls for the result.
        LinkSearch,
        /// Show the candidate list for the chosen book.
        LinkPick,
        /// Pick a synced book to take off StoryGraph (scrollable).
        Remove,
        /// "Remove <title> from StoryGraph?" with Remove / Cancel, plus
        /// "Remove completely" one row below.
        RemoveConfirm,
        /// Second, explicit confirmation for StoryGraph's "Remove book"
        /// (deletes reading history and review).
        RemoveCompletelyConfirm,
        /// Remove running in the background; the tap loop polls for it.
        RemoveRunning,
    }

    /// An in-flight StoryGraph search spawned on a background thread.
    struct SearchPending {
        rx: mpsc::Receiver<Result<Vec<Book>, String>>,
        label: String,
    }

    /// An in-flight "Remove from StoryGraph" spawned on a background thread.
    struct RemovePending {
        rx: mpsc::Receiver<Result<Unsynced, String>>,
        label: String,
        mode: UnsyncMode,
    }

    /// UI layout + mutable state for one session.
    struct Ui<'a> {
        fb: &'a Fb,
        rows: Vec<crate::ui::BookRow>,
        total: usize,
        synced: usize,
        last_sync: Option<i64>,
        cookies_set: bool,
        note: Option<String>, // transient status line (e.g. sync result)
        screen: Screen,
        list_offset: usize, // book-list scroll (Main & Link)
        pick_offset: usize, // candidate-list scroll (LinkPick)
        link_key: Option<String>,
        candidates: Vec<Candidate>,
        search: Option<SearchPending>,
        remove_key: Option<String>,
        removing: Option<RemovePending>,
    }

    impl<'a> Ui<'a> {
        fn button_row(&self) -> i16 {
            (self.fb.rows as i16 - BUTTON_OFFSET_FROM_BOTTOM).max(BOOK_START_ROW + 1)
        }
        fn util_row(&self) -> i16 {
            (self.fb.rows as i16 - UTIL_OFFSET_FROM_BOTTOM).max(BOOK_START_ROW + 2)
        }
        fn hint_row(&self) -> i16 {
            self.fb.rows as i16 - HINT_OFFSET_FROM_BOTTOM
        }
        /// Book slots visible (2 rows each), reserving one row for the
        /// down-scroll indicator.
        fn visible_books(&self) -> usize {
            let area = (self.button_row() - BOOK_START_ROW - 1).max(0) as usize;
            area / 2
        }
        /// Row of the down-scroll indicator (immediately after the last book
        /// slot, one row above the action buttons).
        fn down_nav_row(&self) -> i16 {
            BOOK_START_ROW + (self.visible_books() as i16) * 2
        }
        /// Candidate slots visible (1 row each), reserving one nav row.
        fn cand_visible(&self) -> usize {
            (self.button_row() - BOOK_START_ROW - 1).max(0) as usize
        }

        fn draw(&self, flash: bool) {
            self.fb.cls();
            match self.screen {
                Screen::Main => self.draw_main(),
                Screen::Settings => self.draw_settings(),
                Screen::Link => self.draw_link(),
                Screen::LinkSearch => self.draw_link_search(),
                Screen::LinkPick => self.draw_link_pick(),
                Screen::Remove => self.draw_remove(),
                Screen::RemoveConfirm => self.draw_remove_confirm(),
                Screen::RemoveCompletelyConfirm => self.draw_remove_completely_confirm(),
                Screen::RemoveRunning => self.draw_remove_running(),
            }
            self.fb.refresh(flash);
        }

        // ── Main screen ─────────────────────────────────────────────────────

        fn draw_main(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );

            let last = match self.last_sync {
                Some(ts) => format!("{} {}", t("Last sync:", "Letzter Sync:"), rel_ago(ts)),
                None => t("Not synced yet", "Noch nicht synchronisiert").to_owned(),
            };
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(&last, fb.cols.saturating_sub(2)),
                false,
                false,
            );
            fb.print(
                STATUS_ROW_2,
                2,
                &pad_right(
                    &format!(
                        "{} {} / {}",
                        t("Synced:", "Synchronisiert:"),
                        self.synced,
                        self.total
                    ),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
            let session = if self.cookies_set {
                t("Session: set", "Sitzung: gesetzt")
            } else {
                t("Session: not set", "Sitzung: nicht gesetzt")
            };
            fb.print(
                STATUS_ROW_3,
                2,
                &pad_right(session, fb.cols.saturating_sub(2)),
                false,
                false,
            );

            self.draw_book_list();

            // Action row: sync left, exit right.
            let btn = self.button_row();
            fb.print(
                btn,
                2,
                &t("  [ Sync now ]", "  [ Sync jetzt ]"),
                false,
                false,
            );
            let exit_label = t(" [ Exit ]", " [ Beenden ]");
            let exit_col = self.fb.cols as i16 - exit_label.chars().count() as i16 - 2;
            fb.print(btn, exit_col.max(0), exit_label, false, false);

            // Util row: settings left, link right.
            let util = self.util_row();
            fb.print(
                util,
                2,
                &t("  [ Settings ]", "  [ Einstellungen ]"),
                false,
                false,
            );
            let link_label = t(" [ Link ]", " [ Verknüpfen ]");
            let link_col = self.fb.cols as i16 - link_label.chars().count() as i16 - 2;
            fb.print(util, link_col.max(0), link_label, false, false);

            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t(
                        "Book = sync   ^ = up   v = down",
                        "Buch = Sync   ^ = hoch   v = runter",
                    ),
                    self.fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            self.draw_note();
        }

        // ── Shared book list (Main, Link & Remove) ──────────────────────────

        /// Indices into `self.rows` of the books the current screen lists:
        /// only the synced ones on the Remove screen, all books elsewhere.
        fn list(&self) -> Vec<usize> {
            if self.screen == Screen::Remove {
                super::shelf_indices(&self.rows)
            } else {
                (0..self.rows.len()).collect()
            }
        }

        /// The row index behind a tap on grid row `row` of the book list.
        fn book_at(&self, row: i16) -> Option<usize> {
            if row < BOOK_START_ROW || row >= self.down_nav_row() {
                return None;
            }
            let pos = self.list_offset + ((row - BOOK_START_ROW) / 2) as usize;
            self.list().get(pos).copied()
        }

        /// Draws the section header (with pagination + up indicator), the
        /// visible book slice, and the down-scroll indicator. Uses
        /// `self.list_offset`.
        fn draw_book_list(&self) {
            let fb = self.fb;
            let list = self.list();
            let total = list.len();
            let visible = self.visible_books();
            let max_offset = total.saturating_sub(visible);
            let up = self.list_offset > 0;
            let down = self.list_offset < max_offset;

            let mut header = String::new();
            if up {
                header.push_str("^ ");
            }
            header.push_str(t("BOOKS", "BÜCHER"));
            if total > 0 {
                let first = self.list_offset + 1;
                let last = (self.list_offset + visible).min(total);
                header.push_str(&format!(" ({first}-{last}/{total})"));
            }
            fb.print(
                SECTION_ROW,
                2,
                &pad_right(&header, fb.cols.saturating_sub(2)),
                up,
                false,
            );

            let mut r = BOOK_START_ROW;
            for &i in list.iter().skip(self.list_offset).take(visible) {
                let book = &self.rows[i];
                let title = pad_right(&book.title, fb.cols.saturating_sub(2));
                let mut meta = format!(" {:>3.0}%", book.percent);
                if !book.author.is_empty() {
                    meta.push_str(&format!("  {}", truncate_chars(&book.author, 24)));
                }
                if book.linked {
                    meta.push_str("  [linked]");
                } else if book.allowlisted {
                    meta.push_str("  [sync]");
                }
                let meta = pad_right(&meta, fb.cols.saturating_sub(2));
                fb.print(r, 2, &title, book.allowlisted, false);
                fb.print(r + 1, 2, &meta, book.allowlisted, false);
                r += 2;
            }

            if down {
                let n = total - (self.list_offset + visible);
                let label = if de() {
                    format!("  v {} weitere", n)
                } else {
                    format!("  v {} more", n)
                };
                fb.print(
                    self.down_nav_row(),
                    2,
                    &pad_right(&label, fb.cols.saturating_sub(2)),
                    true,
                    false,
                );
            }
        }

        fn draw_note(&self) {
            if let Some(note) = &self.note {
                self.fb.print(
                    SECTION_ROW,
                    2,
                    &pad_right(note, self.fb.cols.saturating_sub(2)),
                    true,
                    false,
                );
            }
        }

        // ── Settings screen ─────────────────────────────────────────────────

        fn draw_settings(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(
                    &t("Settings (read-only)", "Einstellungen (nur lesen)"),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            let lines = super::settings_lines(&CONFIG);
            let mut r = SECTION_ROW;
            for line in &lines {
                if r >= self.button_row() {
                    break;
                }
                fb.print(
                    r,
                    2,
                    &pad_right(line, fb.cols.saturating_sub(2)),
                    false,
                    false,
                );
                r += 1;
            }

            fb.print(
                self.button_row(),
                2,
                &t(" [ Back ]", " [ Zurück ]"),
                false,
                false,
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t("Read-only - tap Back", "Nur lesen - Zurück antippen"),
                    self.fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
        }

        // ── Link screens ────────────────────────────────────────────────────

        fn draw_link(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(
                    &t("Link a book", "Buch verknüpfen"),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
            fb.print(
                STATUS_ROW_2,
                2,
                &pad_right(
                    &t(
                        "Tap a book to search StoryGraph",
                        "Buch antippen = StoryGraph-Suche",
                    ),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            self.draw_book_list();

            fb.print(
                self.button_row(),
                2,
                &t(" [ Back ]", " [ Zurück ]"),
                false,
                false,
            );
            // Hidden with sync_scope=all: the next pass would re-attach the
            // book, so `unsync_book` refuses it anyway.
            if !CONFIG.sync_all() {
                let remove_label = t(" [ Remove ]", " [ Entfernen ]");
                let remove_col = fb.cols as i16 - remove_label.chars().count() as i16 - 2;
                fb.print(
                    self.button_row(),
                    remove_col.max(0),
                    remove_label,
                    false,
                    false,
                );
            }
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t(
                        "Book = search   ^ = up   v = down",
                        "Buch = Suche   ^ = hoch   v = runter",
                    ),
                    self.fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            self.draw_note();
        }

        fn draw_link_search(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_2,
                2,
                &pad_right(&t("Searching...", "Suchen..."), fb.cols.saturating_sub(2)),
                false,
                false,
            );
            let label = self
                .search
                .as_ref()
                .map(|s| s.label.clone())
                .unwrap_or_default();
            fb.print(
                STATUS_ROW_3,
                2,
                &pad_right(
                    &truncate_chars(&label, fb.cols.saturating_sub(2)),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
            fb.print(
                self.button_row(),
                2,
                &t(" [ Cancel ]", " [ Abbrechen ]"),
                false,
                false,
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t("Waiting for StoryGraph", "Warte auf StoryGraph"),
                    self.fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
        }

        fn draw_link_pick(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );

            let book_title = self
                .link_key
                .as_ref()
                .and_then(|k| self.rows.iter().find(|b| &b.cde_key == k))
                .map(|b| b.title.clone())
                .unwrap_or_default();
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(
                    &format!(
                        "{} {}",
                        t("Link:", "Verknüpfen:"),
                        truncate_chars(&book_title, fb.cols.saturating_sub(10))
                    ),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            let total = self.candidates.len();
            let visible = self.cand_visible();
            let max_offset = total.saturating_sub(visible);
            let up = self.pick_offset > 0;
            let down = self.pick_offset < max_offset;

            let mut header = String::new();
            if up {
                header.push_str("^ ");
            }
            header.push_str(t("RESULTS", "TREFFER"));
            if total > 0 {
                let first = self.pick_offset + 1;
                let last = (self.pick_offset + visible).min(total);
                header.push_str(&format!(" ({first}-{last}/{total})"));
            }
            fb.print(
                SECTION_ROW,
                2,
                &pad_right(&header, fb.cols.saturating_sub(2)),
                up,
                false,
            );

            let mut r = BOOK_START_ROW;
            for i in self.pick_offset..(self.pick_offset + visible).min(total) {
                let c = &self.candidates[i];
                let label = if c.author.is_empty() || c.author == "Unknown Author" {
                    c.title.clone()
                } else {
                    format!("{} - {}", c.title, c.author)
                };
                fb.print(
                    r,
                    2,
                    &pad_right(&label, fb.cols.saturating_sub(2)),
                    false,
                    false,
                );
                r += 1;
            }

            if down {
                let n = total - (self.pick_offset + visible);
                let label = if de() {
                    format!("  v {} weitere", n)
                } else {
                    format!("  v {} more", n)
                };
                fb.print(
                    self.button_row() - 1,
                    2,
                    &pad_right(&label, fb.cols.saturating_sub(2)),
                    true,
                    false,
                );
            }

            fb.print(
                self.button_row(),
                2,
                &t(" [ Back ]", " [ Zurück ]"),
                false,
                false,
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t("Tap a match to link", "Treffer antippen = verknüpfen"),
                    self.fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
        }

        // ── Remove screens ──────────────────────────────────────────────────

        fn draw_remove(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(
                    t("Remove from StoryGraph", "Von StoryGraph entfernen"),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
            fb.print(
                STATUS_ROW_2,
                2,
                &pad_right(
                    t("Tap a synced book", "Synchronisiertes Buch antippen"),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            self.draw_book_list();
            if self.list().is_empty() {
                fb.print(
                    BOOK_START_ROW,
                    2,
                    &pad_right(
                        t("(no synced books)", "(keine synchronisierten Bücher)"),
                        fb.cols.saturating_sub(2),
                    ),
                    false,
                    false,
                );
            }

            fb.print(
                self.button_row(),
                2,
                t(" [ Back ]", " [ Zurück ]"),
                false,
                false,
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t(
                        "Book = remove   ^ = up   v = down",
                        "Buch = entfernen   ^ = hoch   v = runter",
                    ),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );

            self.draw_note();
        }

        fn draw_remove_confirm(&self) {
            let fb = self.fb;
            let width = fb.cols.saturating_sub(2);
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(
                    t("Remove from StoryGraph?", "Von StoryGraph entfernen?"),
                    width,
                ),
                false,
                false,
            );
            fb.print(
                STATUS_ROW_3,
                2,
                &pad_right(&self.remove_title(), width),
                true,
                false,
            );
            let lines: [&str; 3] = if de() {
                [
                    "Setzt das Buch auf StoryGraph auf",
                    "\"to-read\" und synchronisiert es",
                    "nicht mehr. Braucht WLAN.",
                ]
            } else {
                [
                    "Sets the book to \"to-read\" on",
                    "StoryGraph and stops syncing it.",
                    "Needs Wi-Fi.",
                ]
            };
            for (i, line) in lines.iter().enumerate() {
                fb.print(
                    SECTION_ROW + i as i16,
                    2,
                    &pad_right(line, width),
                    false,
                    false,
                );
            }

            let btn = self.button_row();
            fb.print(btn, 2, t(" [ Remove ]", " [ Entfernen ]"), false, false);
            let cancel_label = t(" [ Cancel ]", " [ Abbrechen ]");
            let cancel_col = fb.cols as i16 - cancel_label.chars().count() as i16 - 2;
            fb.print(btn, cancel_col.max(0), cancel_label, false, false);
            fb.print(
                self.util_row(),
                2,
                t(" [ Remove completely... ]", " [ Komplett entfernen... ]"),
                false,
                false,
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t(
                        "Completely = also delete history + review",
                        "Komplett = auch Verlauf + Rezension löschen",
                    ),
                    width,
                ),
                false,
                false,
            );
        }

        fn draw_remove_completely_confirm(&self) {
            let fb = self.fb;
            let width = fb.cols.saturating_sub(2);
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_1,
                2,
                &pad_right(t("Remove COMPLETELY?", "KOMPLETT entfernen?"), width),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_3,
                2,
                &pad_right(&self.remove_title(), width),
                true,
                false,
            );
            let lines: [&str; 4] = if de() {
                [
                    "Entfernt das Buch von StoryGraph und",
                    "LÖSCHT dort Leseverlauf (Tagebuch)",
                    "UND deine Rezension. Nicht",
                    "rückgängig zu machen. Braucht WLAN.",
                ]
            } else {
                [
                    "Removes the book from StoryGraph and",
                    "DELETES its reading history (journal)",
                    "AND your review there. This cannot",
                    "be undone. Needs Wi-Fi.",
                ]
            };
            for (i, line) in lines.iter().enumerate() {
                fb.print(
                    SECTION_ROW + i as i16,
                    2,
                    &pad_right(line, width),
                    false,
                    false,
                );
            }

            let btn = self.button_row();
            fb.print(btn, 2, t(" [ Delete ]", " [ Löschen ]"), false, false);
            let cancel_label = t(" [ Cancel ]", " [ Abbrechen ]");
            let cancel_col = fb.cols as i16 - cancel_label.chars().count() as i16 - 2;
            fb.print(btn, cancel_col.max(0), cancel_label, false, false);
        }

        fn draw_remove_running(&self) {
            let fb = self.fb;
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );
            fb.print(
                STATUS_ROW_2,
                2,
                &pad_right(t("Removing...", "Entferne..."), fb.cols.saturating_sub(2)),
                false,
                false,
            );
            let label = self
                .removing
                .as_ref()
                .map(|p| p.label.clone())
                .unwrap_or_default();
            fb.print(
                STATUS_ROW_3,
                2,
                &pad_right(&label, fb.cols.saturating_sub(2)),
                false,
                false,
            );
            fb.print(
                self.button_row(),
                2,
                t(" [ Back ]", " [ Zurück ]"),
                false,
                false,
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(
                    t(
                        "Back = keeps running in background",
                        "Zurück = läuft im Hintergrund weiter",
                    ),
                    fb.cols.saturating_sub(2),
                ),
                false,
                false,
            );
        }

        // ── Tap handling ────────────────────────────────────────────────────

        fn tap(&mut self, x: i32, y: i32) -> Tap {
            let (x, y) = self.fb.map_touch(x, y);
            let row = ((y - self.fb.view_vert_origin as i32).max(0) / self.fb.fh as i32) as i16;
            let col = (x.max(0) / self.fb.fw as i32) as i16;
            match self.screen {
                Screen::Main => self.tap_main(row, col),
                Screen::Settings => self.tap_settings(row, col),
                Screen::Link => self.tap_link(row, col),
                Screen::LinkSearch => self.tap_link_search(row, col),
                Screen::LinkPick => self.tap_link_pick(row, col),
                Screen::Remove => self.tap_remove(row, col),
                Screen::RemoveConfirm => self.tap_remove_confirm(row, col),
                Screen::RemoveCompletelyConfirm => self.tap_remove_completely_confirm(row, col),
                Screen::RemoveRunning => self.tap_remove_running(row, col),
            }
        }

        /// Scroll the book list up one page; returns true when it moved.
        fn scroll_up(&mut self) -> bool {
            if self.list_offset == 0 {
                return false;
            }
            let vis = self.visible_books();
            self.list_offset = self.list_offset.saturating_sub(vis);
            true
        }

        /// Scroll the book list down one page; returns true when it moved.
        fn scroll_down(&mut self) -> bool {
            let vis = self.visible_books();
            let max = self.list().len().saturating_sub(vis);
            if self.list_offset >= max {
                return false;
            }
            self.list_offset = (self.list_offset + vis).min(max);
            true
        }

        fn tap_main(&mut self, row: i16, col: i16) -> Tap {
            let half = self.fb.cols as i16 / 2;
            if row == self.button_row() {
                if col >= half {
                    return Tap::Exit;
                }
                match self.sync_now() {
                    Ok(summary) => self.note = Some(summary),
                    Err(e) => self.note = Some(format!("{e}")),
                }
                return Tap::Redraw;
            }
            if row == self.util_row() {
                if col >= half {
                    self.screen = Screen::Link;
                    self.list_offset = 0;
                    self.note = None;
                } else {
                    self.screen = Screen::Settings;
                    self.note = None;
                }
                return Tap::Redraw;
            }
            if row == SECTION_ROW && self.scroll_up() {
                return Tap::Redraw;
            }
            if row == self.down_nav_row() && self.scroll_down() {
                return Tap::Redraw;
            }
            if let Some(idx) = self.book_at(row) {
                let key = self.rows[idx].cde_key.clone();
                let allowed =
                    toggle_allowlist(Path::new(&CONFIG.allowlist_path), &key).unwrap_or(false);
                self.rows[idx].allowlisted = allowed;
                self.synced = self.rows.iter().filter(|b| b.allowlisted).count();
                self.note = None;
                return Tap::Redraw;
            }
            Tap::None
        }

        fn tap_settings(&mut self, row: i16, _col: i16) -> Tap {
            if row == self.button_row() || row == self.util_row() {
                self.screen = Screen::Main;
                self.note = None;
                return Tap::Redraw;
            }
            Tap::None
        }

        fn tap_link(&mut self, row: i16, col: i16) -> Tap {
            if row == self.button_row() {
                if col >= self.fb.cols as i16 / 2 && !CONFIG.sync_all() {
                    if self.removing.is_some() {
                        self.note = Some(
                            t("A removal is still running", "Entfernen läuft noch").to_owned(),
                        );
                    } else {
                        self.screen = Screen::Remove;
                        self.list_offset = 0;
                        self.note = None;
                    }
                    return Tap::Redraw;
                }
                self.screen = Screen::Main;
                self.note = None;
                return Tap::Redraw;
            }
            if row == SECTION_ROW && self.scroll_up() {
                return Tap::Redraw;
            }
            if row == self.down_nav_row() && self.scroll_down() {
                return Tap::Redraw;
            }
            if let Some(idx) = self.book_at(row) {
                self.start_link_search(idx);
                return Tap::Redraw;
            }
            Tap::None
        }

        fn tap_remove(&mut self, row: i16, _col: i16) -> Tap {
            if row == self.button_row() {
                self.screen = Screen::Link;
                self.list_offset = 0;
                self.note = None;
                return Tap::Redraw;
            }
            if row == SECTION_ROW && self.scroll_up() {
                return Tap::Redraw;
            }
            if row == self.down_nav_row() && self.scroll_down() {
                return Tap::Redraw;
            }
            if let Some(idx) = self.book_at(row) {
                self.remove_key = Some(self.rows[idx].cde_key.clone());
                self.screen = Screen::RemoveConfirm;
                self.note = None;
                return Tap::Redraw;
            }
            Tap::None
        }

        fn tap_remove_confirm(&mut self, row: i16, col: i16) -> Tap {
            if row == self.util_row() {
                // Never removes directly: the irreversible variant has its
                // own confirmation screen.
                self.screen = Screen::RemoveCompletelyConfirm;
                return Tap::Redraw;
            }
            if row != self.button_row() {
                return Tap::None;
            }
            if col < self.fb.cols as i16 / 2 {
                self.start_remove(UnsyncMode::ToRead);
            } else {
                self.screen = Screen::Remove;
            }
            Tap::Redraw
        }

        fn tap_remove_completely_confirm(&mut self, row: i16, col: i16) -> Tap {
            if row != self.button_row() {
                return Tap::None;
            }
            if col < self.fb.cols as i16 / 2 {
                self.start_remove(UnsyncMode::Completely);
            } else {
                self.screen = Screen::RemoveConfirm;
            }
            Tap::Redraw
        }

        fn tap_remove_running(&mut self, row: i16, _col: i16) -> Tap {
            if row == self.button_row() {
                // The removal keeps running; `drain_remove` applies its result
                // on whichever screen is showing when it arrives.
                self.screen = Screen::Main;
                self.list_offset = 0;
                self.note = Some(
                    t(
                        "Removing in the background...",
                        "Entfernen läuft im Hintergrund...",
                    )
                    .to_owned(),
                );
                return Tap::Redraw;
            }
            Tap::None
        }

        /// The title of the book a removal is about, for the screens.
        fn remove_title(&self) -> String {
            self.remove_key
                .as_ref()
                .and_then(|k| self.rows.iter().find(|b| &b.cde_key == k))
                .map(|b| b.title.clone())
                .unwrap_or_default()
        }

        /// Run `unsync_book` on a background thread (network: the tap loop
        /// must stay responsive, and the HTTP client has no global timeout).
        fn start_remove(&mut self, mode: UnsyncMode) {
            let Some(key) = self.remove_key.clone() else {
                self.screen = Screen::Remove;
                return;
            };
            let label = self.remove_title();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let res = unsync_book(
                    &RealStoryGraph,
                    &key,
                    Path::new(&CONFIG.state_path),
                    Path::new(&CONFIG.allowlist_path),
                    CONFIG.sync_all(),
                    mode,
                )
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send(res);
            });
            self.removing = Some(RemovePending { rx, label, mode });
            self.screen = Screen::RemoveRunning;
            self.note = None;
        }

        /// Poll the in-flight removal; on completion update rows, note and
        /// screen and return true (caller redraws).
        fn drain_remove(&mut self) -> bool {
            let result = match &self.removing {
                Some(p) => p.rx.try_recv(),
                None => return false,
            };
            let res = match result {
                Ok(res) => res,
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(t("Remove failed (see log)", "Entfernen fehlgeschlagen (Log)").to_owned())
                }
            };
            let (label, mode) = self
                .removing
                .take()
                .map(|p| (p.label, p.mode))
                .unwrap_or((String::new(), UnsyncMode::ToRead));
            let running = self.screen == Screen::RemoveRunning;
            match res {
                Ok(done) => {
                    // The manual mapping is kept on purpose, so the book may
                    // still count as linked.
                    let mappings = load_manual_mappings(Path::new(&CONFIG.manual_mappings_path));
                    if let Some(row) = self.rows.iter_mut().find(|b| b.cde_key == done.cde_key) {
                        row.on_shelf = false;
                        row.allowlisted = false;
                        row.linked = mappings.contains_key(&done.cde_key);
                    }
                    self.synced = self.rows.iter().filter(|b| b.allowlisted).count();
                    if let Ok(state) = State::load(Path::new(&CONFIG.state_path)) {
                        self.last_sync = state.books.values().map(|b| b.last_synced_at).max();
                    }
                    if let Err(e) = menu::write_menu(Path::new(&CONFIG.cc_db_path)) {
                        eprintln!("warning: could not update the KUAL menu: {e:#}");
                    }
                    let done_label = match mode {
                        UnsyncMode::ToRead => t("Removed:", "Entfernt:"),
                        UnsyncMode::Completely => t("Deleted:", "Gelöscht:"),
                    };
                    self.note = Some(format!("{done_label} {label}"));
                    if running {
                        self.screen = Screen::Main;
                        self.list_offset = 0;
                    }
                }
                Err(e) => {
                    eprintln!("remove from StoryGraph failed: {e}");
                    self.note = Some(classify_network_error(&e));
                    if running {
                        self.screen = Screen::Remove;
                        self.list_offset = 0;
                    }
                }
            }
            true
        }

        fn tap_link_search(&mut self, row: i16, _col: i16) -> Tap {
            if row == self.button_row() {
                // Drop the receiver; the background thread's send then fails
                // harmlessly (it is killed when the process exits).
                self.search = None;
                self.screen = Screen::Link;
                self.note = None;
                return Tap::Redraw;
            }
            Tap::None
        }

        fn tap_link_pick(&mut self, row: i16, _col: i16) -> Tap {
            if row == self.button_row() {
                self.screen = Screen::Link;
                self.note = None;
                return Tap::Redraw;
            }
            if row == SECTION_ROW && self.pick_offset > 0 {
                let vis = self.cand_visible();
                self.pick_offset = self.pick_offset.saturating_sub(vis);
                return Tap::Redraw;
            }
            if row == self.button_row() - 1 {
                let vis = self.cand_visible();
                let max = self.candidates.len().saturating_sub(vis);
                if self.pick_offset < max {
                    self.pick_offset = (self.pick_offset + vis).min(max);
                    return Tap::Redraw;
                }
                return Tap::None;
            }
            if row >= BOOK_START_ROW && row < self.button_row() - 1 {
                let idx = self.pick_offset + (row - BOOK_START_ROW) as usize;
                if idx < self.candidates.len() {
                    let id = self.candidates[idx].id.clone();
                    let Some(key) = self.link_key.clone() else {
                        return Tap::None;
                    };
                    match add_manual_mapping(Path::new(&CONFIG.manual_mappings_path), &key, &id) {
                        Ok(()) => {
                            self.mark_linked(&key);
                            self.note = Some(t("Linked", "Verknüpft").to_owned());
                            self.screen = Screen::Main;
                            self.list_offset = 0;
                        }
                        Err(e) => {
                            self.note = Some(format!("{e}"));
                            self.screen = Screen::Link;
                        }
                    }
                    if let Err(e) = menu::write_menu(Path::new(&CONFIG.cc_db_path)) {
                        eprintln!("warning: could not update the KUAL menu: {e:#}");
                    }
                    return Tap::Redraw;
                }
            }
            Tap::None
        }

        /// Mark the just-linked book as linked in the display rows.
        fn mark_linked(&mut self, key: &str) {
            if let Some(row) = self.rows.iter_mut().find(|b| b.cde_key == key) {
                row.linked = true;
            }
        }

        /// Kick off a background StoryGraph search for the book at `idx`.
        fn start_link_search(&mut self, idx: usize) {
            let book = &self.rows[idx];
            let key = book.cde_key.clone();
            let query = if book.author.is_empty() {
                book.title.clone()
            } else {
                format!("{} {}", book.title, book.author)
            };
            let label = book.title.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let res = crate::storygraph::search(&query).map_err(|e| format!("{e:#}"));
                let _ = tx.send(res);
            });
            self.link_key = Some(key);
            self.search = Some(SearchPending { rx, label });
            self.screen = Screen::LinkSearch;
            self.note = None;
        }

        /// Poll the in-flight search channel; on completion update the screen
        /// and return true (caller redraws).
        fn drain_search(&mut self) -> bool {
            let result = match &self.search {
                Some(p) => p.rx.try_recv(),
                None => return false,
            };
            let Ok(res) = result else {
                return false;
            };
            // Drop the receiver; the background thread's send then fails
            // harmlessly and the thread is killed when the process exits.
            self.search = None;
            match res {
                Ok(books) if books.is_empty() => {
                    self.note = Some(t("No results", "Keine Treffer").to_owned());
                    self.screen = Screen::Link;
                }
                Ok(books) => {
                    self.candidates = books
                        .into_iter()
                        .take(10)
                        .map(|b| Candidate {
                            id: b.id,
                            title: b.title,
                            author: b.author,
                        })
                        .collect();
                    self.pick_offset = 0;
                    self.screen = Screen::LinkPick;
                }
                Err(e) => {
                    self.note = Some(classify_network_error(&e));
                    self.screen = Screen::Link;
                }
            }
            true
        }

        fn sync_now(&mut self) -> Result<String> {
            self.note = Some(t("Syncing...", "Synchronisiere...").to_owned());
            self.draw(false);
            let cc_db = Path::new(&CONFIG.cc_db_path);
            let state_path = Path::new(&CONFIG.state_path);
            match daemon::sync_once_from_paths(cc_db, state_path, false) {
                Ok(outcomes) => {
                    // Refresh the synced count after the pass.
                    if let Ok(reader) = Reader::open_read_only(CONFIG.cc_db_path.as_str()) {
                        if let Ok(snap) =
                            reader.snapshot(CONFIG.finish_threshold, CONFIG.min_progress)
                        {
                            let allowlist = load_allowlist(Path::new(&CONFIG.allowlist_path));
                            self.synced = snap
                                .iter()
                                .filter(|b| allowlist.contains(&b.cde_key))
                                .count();
                            self.total = snap.len();
                        }
                    }
                    if let Ok(state) = State::load(state_path) {
                        self.last_sync = state.books.values().map(|b| b.last_synced_at).max();
                    }
                    self.note = None;
                    let changes = outcomes.iter().filter(|o| o.is_change()).count();
                    let failed = outcomes
                        .iter()
                        .filter(|o| matches!(o, crate::sync::Outcome::Failed { .. }))
                        .count();
                    let summary = match (CONFIG.language.eq_ignore_ascii_case("de"), failed) {
                        (true, 0) => format!("Sync fertig: {changes} Änderungen"),
                        (true, f) => format!("Sync: {changes} Änderungen, {f} Fehler (Log)"),
                        (false, 0) => format!("Sync done: {changes} change(s)"),
                        (false, f) => format!("Sync: {changes} change(s), {f} failed (log)"),
                    };
                    Ok(summary)
                }
                Err(e) => {
                    self.note = None;
                    Err(e.context("sync failed"))
                }
            }
        }
    }

    pub fn run() -> Result<()> {
        let fb = Fb::init()?;

        // Load data; on failure, show the error and wait for a tap.
        let load = || -> Result<(Vec<crate::ui::BookRow>, usize, usize, Option<i64>, bool)> {
            let reader =
                Reader::open_read_only(CONFIG.cc_db_path.as_str()).context("opening cc.db")?;
            let snapshot = reader.snapshot(CONFIG.finish_threshold, CONFIG.min_progress)?;
            let allowlist = load_allowlist(Path::new(&CONFIG.allowlist_path));
            let state = State::load(Path::new(&CONFIG.state_path)).unwrap_or_default();
            let linked = menu::linked_keys(Path::new(&CONFIG.manual_mappings_path), &state);
            let on_shelf = state.books.keys().cloned().collect();
            let rows = crate::ui::build_rows(&snapshot, &allowlist, &linked, &on_shelf);
            let total = snapshot.len();
            let synced = rows.iter().filter(|b| b.allowlisted).count();
            let last_sync = state.books.values().map(|b| b.last_synced_at).max();
            let cookies_set = !CONFIG.session_cookie.is_empty();
            Ok((rows, total, synced, last_sync, cookies_set))
        };

        let mut ui = match load() {
            Ok((rows, total, synced, last_sync, cookies_set)) => Ui {
                fb: &fb,
                rows,
                total,
                synced,
                last_sync,
                cookies_set,
                note: None,
                screen: Screen::Main,
                list_offset: 0,
                pick_offset: 0,
                link_key: None,
                candidates: Vec::new(),
                search: None,
                remove_key: None,
                removing: None,
            },
            Err(e) => {
                show_error(&fb, &format!("{e:#}"));
                return Ok(());
            }
        };

        ui.draw(true);

        // Input loop. The touch fd is made non-blocking so the loop can also
        // poll for a completed background search instead of freezing on a
        // blocking read while the network is busy.
        let touch_fd = open_touchscreen()?;
        unsafe {
            let flags = libc::fcntl(touch_fd, libc::F_GETFL);
            if flags >= 0 {
                libc::fcntl(touch_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
        let mut touching = false;
        let mut was_touching = false;
        let mut x = 0i32;
        let mut y = 0i32;

        loop {
            let searched = ui.drain_search();
            let removed = ui.drain_remove();
            if searched || removed {
                ui.draw(false);
            }

            let mut ev: InputEvent = unsafe { std::mem::zeroed() };
            let n = unsafe {
                libc::read(
                    touch_fd,
                    &mut ev as *mut InputEvent as *mut c_void,
                    std::mem::size_of::<InputEvent>(),
                )
            };
            if n != std::mem::size_of::<InputEvent>() as isize {
                // No event pending (non-blocking read); brief sleep so the
                // loop doesn't busy-spin, then check the search channel again.
                std::thread::sleep(Duration::from_millis(15));
                continue;
            }
            match ev.type_ {
                fb::EV_ABS => match ev.code {
                    fb::ABS_MT_POSITION_X => x = ev.value,
                    fb::ABS_MT_POSITION_Y => y = ev.value,
                    fb::ABS_MT_PRESSURE => {
                        touching = ev.value > 0;
                    }
                    fb::ABS_MT_TRACKING_ID => {
                        touching = ev.value >= 0;
                    }
                    _ => {}
                },
                fb::EV_KEY => match ev.code {
                    fb::BTN_TOUCH => touching = ev.value == 1,
                    _ => {}
                },
                _ => {}
            }
            if was_touching && !touching {
                match ui.tap(x, y) {
                    Tap::Exit => break,
                    Tap::Redraw => ui.draw(false),
                    Tap::None => {}
                }
            }
            was_touching = touching;
        }

        // Restore a clean screen before returning to the launcher.
        fb.cls();
        fb.refresh(true);
        Ok(())
    }

    fn open_touchscreen() -> Result<c_int> {
        let mut dev_count: usize = 0;
        let devs = unsafe {
            fb::fbink_input_scan(
                fb::INPUT_TOUCHSCREEN | fb::INPUT_TABLET,
                0,
                fb::OPEN_BLOCKING | fb::NO_RECAP,
                &mut dev_count,
            )
        };
        if devs.is_null() || dev_count == 0 {
            bail!("no input devices found");
        }
        let mut touch_fd: c_int = -1;
        for i in 0..dev_count {
            let d = unsafe { *devs.add(i) };
            if d.matched && d.fd >= 0 {
                touch_fd = d.fd;
                break;
            }
        }
        unsafe { libc::free(devs as *mut c_void) };
        if touch_fd < 0 {
            bail!("no touchscreen found");
        }
        Ok(touch_fd)
    }

    fn show_error(fb: &Fb, msg: &str) {
        fb.cls();
        fb.print(0, 0, &pad_center("KindleStoryGraph", fb.cols), true, false);
        fb.print(
            2,
            2,
            &pad_right(&t("Error:", "Fehler:"), fb.cols.saturating_sub(2)),
            false,
            false,
        );
        // Wrap the message into roughly cols-wide lines.
        let mut row = 3i16;
        for chunk in msg.as_bytes().chunks(fb.cols.saturating_sub(2).min(80)) {
            let line = String::from_utf8_lossy(chunk);
            fb.print(row, 2, &line, false, false);
            row += 1;
            if row >= fb.rows as i16 - 1 {
                break;
            }
        }
        let hint = t("Tap to close", "Zum Schließen antippen");
        fb.print(fb.rows as i16 - 2, 2, &hint, false, false);
        fb.refresh(true);

        // Wait for a single tap.
        if let Ok(fd) = open_touchscreen() {
            let mut touching = false;
            let mut was = false;
            loop {
                let mut ev: InputEvent = unsafe { std::mem::zeroed() };
                let n = unsafe {
                    libc::read(
                        fd,
                        &mut ev as *mut InputEvent as *mut c_void,
                        std::mem::size_of::<InputEvent>(),
                    )
                };
                if n != std::mem::size_of::<InputEvent>() as isize {
                    continue;
                }
                match ev.type_ {
                    fb::EV_ABS => match ev.code {
                        fb::ABS_MT_PRESSURE => touching = ev.value > 0,
                        fb::ABS_MT_TRACKING_ID => touching = ev.value >= 0,
                        _ => {}
                    },
                    fb::EV_KEY => match ev.code {
                        fb::BTN_TOUCH => touching = ev.value == 1,
                        _ => {}
                    },
                    _ => {}
                }
                if was && !touching {
                    break;
                }
                was = touching;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_rows, pad_center, pad_right, settings_lines, shelf_indices, truncate_chars};
    use crate::reader::{BookProgress, Status};
    use std::collections::{HashMap, HashSet};

    fn book(key: &str, title: &str, author: &str, percent: f64) -> BookProgress {
        BookProgress {
            cde_key: key.to_owned(),
            cde_type: "EBOK".to_owned(),
            title: title.to_owned(),
            author: author.to_owned(),
            percent,
            last_access: 0,
            status: Status::CurrentlyReading,
        }
    }

    #[test]
    fn truncate_short_passthrough_and_long_gets_marker() {
        assert_eq!(truncate_chars("hi", 5), "hi");
        assert_eq!(truncate_chars("hello", 3), "he~");
        // UTF-8 safe: umlauts count as one cell.
        assert_eq!(truncate_chars("hällo", 3), "hä~");
    }

    #[test]
    fn pad_right_fills_to_exact_width() {
        assert_eq!(pad_right("ab", 4), "ab  ");
        assert_eq!(pad_right("abcdef", 3), "ab~");
    }

    #[test]
    fn pad_center_centers() {
        assert_eq!(pad_center("a", 5), "  a  ");
        assert_eq!(pad_center("abcd", 3), "ab~");
    }

    #[test]
    fn build_rows_marks_allowlist_and_linked() {
        let snap = vec![book("K1", "T1", "A1", 42.0), book("K2", "T2", "A2", 90.0)];
        let allowlist: HashSet<String> = ["K1".to_owned()].into_iter().collect();
        let linked: HashMap<String, String> =
            [("K2".to_owned(), "sg-2".to_owned())].into_iter().collect();
        let rows = build_rows(&snap, &allowlist, &linked, &HashSet::new());
        assert_eq!(rows.len(), 2);
        assert!(rows[0].allowlisted);
        assert!(!rows[0].linked);
        assert!(!rows[1].allowlisted);
        assert!(rows[1].linked);
        assert_eq!(rows[0].percent, 42.0);
        assert_eq!(rows[0].title, "T1");
    }

    #[test]
    fn settings_lines_show_config_values() {
        let cfg = crate::config::Config {
            min_progress: 2.0,
            finish_threshold: 99.5,
            language: "de".to_owned(),
            sync_scope: "allowlist".to_owned(),
            cc_db_path: "/a/cc.db".to_owned(),
            state_path: "/a/state.json".to_owned(),
            manual_mappings_path: "/a/map.ini".to_owned(),
            allowlist_path: "/a/allow.ini".to_owned(),
            ..crate::config::Config::default()
        };

        let lines = settings_lines(&cfg);
        assert_eq!(lines.len(), 8);
        assert!(lines[0].contains("min_progress"));
        assert!(lines[0].contains("2.0"));
        assert!(lines[1].contains("finish_threshold"));
        assert!(lines[1].contains("99.5"));
        assert!(lines[2].contains("language"));
        assert!(lines[2].contains("de"));
        assert!(lines[3].contains("sync_scope"));
        assert!(lines[3].contains("allowlist"));
        assert!(lines[4].contains("cc.db"));
        assert!(lines[4].contains("/a/cc.db"));
        assert!(lines[5].contains("state"));
        assert!(lines[5].contains("/a/state.json"));
        assert!(lines[6].contains("manual mappings"));
        assert!(lines[6].contains("/a/map.ini"));
        assert!(lines[7].contains("allowlist"));
        assert!(lines[7].contains("/a/allow.ini"));
    }

    #[test]
    fn shelf_indices_lists_only_synced_books() {
        let snap = vec![
            book("K1", "T1", "A1", 42.0),
            book("K2", "T2", "A2", 90.0),
            book("K3", "T3", "A3", 10.0),
        ];
        let on_shelf: HashSet<String> = ["K1".to_owned(), "K3".to_owned()].into_iter().collect();
        let rows = build_rows(&snap, &HashSet::new(), &HashMap::new(), &on_shelf);
        assert!(rows[0].on_shelf);
        assert!(!rows[1].on_shelf);
        assert_eq!(shelf_indices(&rows), vec![0, 2]);
        assert!(shelf_indices(&rows[1..2]).is_empty());
    }
}
