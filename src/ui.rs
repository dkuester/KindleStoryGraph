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
}

/// Build the display rows for the snapshot (pure, host-testable). The badges
/// (`[sync]`, `[linked]`) are folded into a single trailing suffix.
#[cfg_attr(not(all(target_os = "linux", target_arch = "arm")), allow(dead_code))]
pub(crate) fn build_rows(
    snapshot: &[crate::reader::BookProgress],
    allowlist: &std::collections::HashSet<String>,
    linked: &std::collections::HashMap<String, String>,
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
        })
        .collect()
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

    use anyhow::{Context, Result, bail};

    use crate::config::{CONFIG, load_allowlist, toggle_allowlist};
    use crate::daemon;
    use crate::fbink as fb;
    use crate::fbink::{FBInkConfig, FBInkState, InputEvent};
    use crate::menu;
    use crate::reader::Reader;
    use crate::state::State;

    /// Pick a UI string by configured language (German when `language` is "de").
    fn t(en: &'static str, de: &'static str) -> &'static str {
        if CONFIG.language.eq_ignore_ascii_case("de") {
            de
        } else {
            en
        }
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

    const FONT_MULT: u8 = 3;
    const TITLE_ROW: i16 = 0;
    const STATUS_ROW_1: i16 = 2;
    const STATUS_ROW_2: i16 = 3;
    const STATUS_ROW_3: i16 = 4;
    const SECTION_ROW: i16 = 6;
    const BOOK_START_ROW: i16 = 7;
    const BUTTON_OFFSET_FROM_BOTTOM: i16 = 3;
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

    /// UI layout + mutable state for one session.
    struct Ui<'a> {
        fb: &'a Fb,
        rows: Vec<crate::ui::BookRow>,
        total: usize,
        synced: usize,
        last_sync: Option<i64>,
        cookies_set: bool,
        note: Option<String>, // transient status line (e.g. sync result)
    }

    impl<'a> Ui<'a> {
        fn button_row(&self) -> i16 {
            (self.fb.rows as i16 - BUTTON_OFFSET_FROM_BOTTOM).max(BOOK_START_ROW + 1)
        }
        fn hint_row(&self) -> i16 {
            self.fb.rows as i16 - HINT_OFFSET_FROM_BOTTOM
        }
        fn visible_books(&self) -> usize {
            let area = (self.button_row() - BOOK_START_ROW).max(0) as usize;
            area / 2
        }

        fn draw(&self, flash: bool) {
            let fb = self.fb;
            fb.cls();

            // Title bar (full-width, inverted).
            fb.print(
                TITLE_ROW,
                0,
                &pad_center("KindleStoryGraph", fb.cols),
                true,
                false,
            );

            // Status block.
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

            // Section label.
            fb.print(SECTION_ROW, 2, &t("BOOKS", "BÜCHER"), false, false);

            // Book list (2 rows each; allowlisted rows are inverted).
            let mut r = BOOK_START_ROW;
            let vis = self.visible_books();
            for (i, book) in self.rows.iter().enumerate() {
                if i >= vis {
                    break;
                }
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
            // "+N more" if the list is longer than fits.
            if self.rows.len() > vis {
                // simpler localized label:
                let label = if CONFIG.language.eq_ignore_ascii_case("de") {
                    format!("  + {} weitere", self.rows.len() - vis)
                } else {
                    format!("  + {} more", self.rows.len() - vis)
                };
                fb.print(
                    r,
                    2,
                    &pad_right(&label, fb.cols.saturating_sub(2)),
                    false,
                    false,
                );
            }

            // Buttons.
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

            // Hint line.
            let hint = t(
                "Tap a book to toggle sync",
                "Buch antippen = Sync umschalten",
            );
            fb.print(
                self.hint_row(),
                2,
                &pad_right(hint, self.fb.cols.saturating_sub(2)),
                false,
                false,
            );

            // Transient note over the section row area.
            if let Some(note) = &self.note {
                fb.print(
                    SECTION_ROW,
                    2,
                    &pad_right(note, self.fb.cols.saturating_sub(2)),
                    true,
                    false,
                );
            }

            fb.refresh(flash);
        }

        /// Map a touch position (raw panel coords) to an action.
        fn tap(&mut self, x: i32, y: i32) -> Tap {
            let fb = self.fb;
            let (x, y) = fb.map_touch(x, y);
            // Account for the vertical viewport origin before dividing into rows.
            let row = ((y - fb.view_vert_origin as i32).max(0) / fb.fh as i32) as i16;
            let col = (x.max(0) / fb.fw as i32) as i16;

            // Buttons row.
            if row == self.button_row() {
                if col >= self.fb.cols as i16 / 2 {
                    return Tap::Exit;
                }
                // Sync now.
                match self.sync_now() {
                    Ok(summary) => self.note = Some(summary),
                    Err(e) => self.note = Some(format!("{}", e)),
                }
                return Tap::Redraw;
            }

            // Book rows.
            if row >= BOOK_START_ROW && row < self.button_row() {
                let idx = ((row - BOOK_START_ROW) / 2) as usize;
                let vis = self.visible_books();
                if idx < vis && idx < self.rows.len() {
                    let key = self.rows[idx].cde_key.clone();
                    let allowed =
                        toggle_allowlist(Path::new(&CONFIG.allowlist_path), &key).unwrap_or(false);
                    self.rows[idx].allowlisted = allowed;
                    self.synced = self.rows.iter().filter(|b| b.allowlisted).count();
                    self.note = None;
                    return Tap::Redraw;
                }
            }

            Tap::None
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
                    let summary = if CONFIG.language.eq_ignore_ascii_case("de") {
                        format!("Sync fertig: {} Änderungen", outcomes.len())
                    } else {
                        format!("Sync done: {} change(s)", outcomes.len())
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
            let rows = crate::ui::build_rows(&snapshot, &allowlist, &linked);
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
            },
            Err(e) => {
                show_error(&fb, &format!("{e:#}"));
                return Ok(());
            }
        };

        ui.draw(true);

        // Input loop.
        let touch_fd = open_touchscreen()?;
        let mut touching = false;
        let mut was_touching = false;
        let mut x = 0i32;
        let mut y = 0i32;

        loop {
            let mut ev: InputEvent = unsafe { std::mem::zeroed() };
            let n = unsafe {
                libc::read(
                    touch_fd,
                    &mut ev as *mut InputEvent as *mut c_void,
                    std::mem::size_of::<InputEvent>(),
                )
            };
            if n != std::mem::size_of::<InputEvent>() as isize {
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
    use super::{build_rows, pad_center, pad_right, truncate_chars};
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
        let rows = build_rows(&snap, &allowlist, &linked);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].allowlisted);
        assert!(!rows[0].linked);
        assert!(!rows[1].allowlisted);
        assert!(rows[1].linked);
        assert_eq!(rows[0].percent, 42.0);
        assert_eq!(rows[0].title, "T1");
    }
}
