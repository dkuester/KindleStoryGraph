# UI & configuration options — review draft

Status: **proposal, nothing implemented.** Goal: make KindleStoryGraph feel like
NickelStoryGraph instead of a KUAL menu tree.

---

## 1. Where the friction actually comes from

The KUAL menu is not a bad UI that needs better labels — it is a launcher list
with no input widgets. The limits are structural (all verified against KUAL
Next's `src/menu.c` / `src/ui_fbink.c`):

| Constraint | Consequence for us |
|---|---|
| No text input, no dialogs, no scrolling viewport | `session_cookie` / `remember_user_token` can only be pasted into `config.ini` from a PC. Email/password login is impossible on-device. |
| `checked` is press-only, never initial state | On/off state must be encoded in the entry text (`"Title  [sync]"`), which is why the menu reads like a log file. |
| Actions `exec` the action in place (exitmenu default true) | Every tap throws the user out of the menu unless each script opts out. |
| Reload resets navigation to root, page 1 | A 60-book library means scrolling from the top after every toggle. |
| ~250 ms refresh window, one process per action | No progress display, no "syncing…" feedback, no result page. Errors are invisible unless written into a label. |
| No status surface at all | "Did the last sync work?" cannot be answered without reading a log over USB. |

NickelStoryGraph does not have this problem because it is **in** the reader:
a menu entry next to the book, status submenu, link/unlink, confirmation
dialogs, a settings dialog with tabs. On a Kindle there is no hookable reader
UI, so we cannot copy that placement. What we *can* copy is the *shape*:
one control panel with real widgets, plus default-sane behaviour that needs no
panel at all.

## 2. What the firmware gives us (reverse-engineered)

Source: `update_kindle_11th_2024_5.20.1.bin` (KT6 / Kindle Basic 2024,
platform `Bellatrix`, MD5 `9965a4aafbb4ebd98aff680dad1b753b`), unpacked with
kindletool → `rootfs.img` (ext3, 768 MB). Paths below are from that image.

| Finding | Evidence | What it enables |
|---|---|---|
| UI stack is **X11 + awesome WM**, configured in Lua, with named layers (keyboard, dialog, chrome, focus, screensaver) | `/etc/xdg/awesome/lab126_{keyboard,dialog,chrome,focus,screensaver}_layer.lua`, `rc.lua`, `lab126_ligl.lua` | Native apps are X clients; window role is decided by X properties (`N=keyboard`, `N=keyboardExt`), so a foreign app can be placed in the layers. |
| **GTK+ 2.20.1** ships on the device (no GTK 3) | `/usr/lib/libgtk-x11-2.0.so.0.2000.1`, `libgdk-x11-2.0`, `libpango-1.0.so.0.2600.2`, `libcairo.so.2` | A real widget UI (dialogs, entries, check buttons, tree view) is possible without bundling a toolkit. Precedents: KUALA (C++/GTK), kterm, Abiword/Textadept ports. |
| The on-screen keyboard is its own X client, `kb`, started by upstart | `/usr/bin/kb`, `/etc/upstart/kb.conf`, layouts only in `/usr/share/keyboard.sqsh` | The framework apps call it over LIPC (`com.lab126.keyboard`). Kterm and Abiword both **ship their own keyboard**, which is the honest expectation for a GTK app: either implement a keyboard (kterm's `keyboard.c` + `layouts/` is the reference, GPL — licence clash with our code) or avoid text entry. |
| The stock browser is **Chromium**, launched as user `framework` | `/usr/bin/browser` → `exec /usr/bin/chromium/kindle_browser … --enable-grayscale-mode`; engine in `/usr/bin/chromium.sqsh` | A locally served HTML page renders in a modern engine, and **text fields get the system keyboard** — exactly what the menu can't do. |
| Chromium arrived in **5.16.4**, not 5.20 | MobileRead "PW5 the browser in 5.16.4 is chromium" + `Tokisaki-Galaxy/kindle-modify-browserUA` ("5.16.4 and above… Chromium-based"); pre-5.16.4 browser is WebKit (`mesquite … file:///var/local/mesquite/browser/`) and Chromium dropped `file://` support | The **O3 floor is 5.16.4**, so a 2022 Paperwhite (11th gen) has Chromium too — the frontend does not need a WebKit/ES5 fallback unless we go below 5.16.4. Note the 5.16.x launcher uses `chroot /chroot … --single-process`, 5.20 uses direct `exec` — the launch script has to cover both variants. |
| Web apps ("WAFs") are installed as folders with a `config.xml` and registered in `/var/local/appreg.db` | `/opt/var/local/mesquite/<app>/config.xml` (store, browser, webview, help, kft, ku, …), `/var/local/appreg.db` strings in `/usr/lib/libappreg.so`, copy step in `/etc/upstart/appmgrd.conf` | A WAF can appear as a real app. WAF runtime is likely WebKitGTK (`/usr/lib/webkit-gtk.sqsh`) — i.e. an old engine, not Chromium. |
| Any registered app is started with one LIPC call | `/opt/var/local/mesquite/start.sh`: `lipc-set-prop com.lab126.appmgrd start app://com.lab126.$APP` | A KUAL action can open our own UI — browser page or WAF — with a one-line script. |
| A generic content-window WAF exists (`com.lab126.webview`) | `/opt/var/local/mesquite/webview/{config.xml,index.html}` | Possible shortcut to show web content inside Kindle chrome without writing a full WAF. Needs a params check on-device. |

Consequences worth stating plainly: nothing here gets us *inside* the reader.
Every option below is "a second screen you open", not "a menu next to the book".

## 3. What has to be configurable, and how often

| Setting | Changed | Needs on-device UI? |
|---|---|---|
| StoryGraph session (`session_cookie`, `remember_user_token`) | once, then on expiry | Once: password login or cookie import. Expiry is the recurring pain. |
| Per-book sync allowlist | continuously | **Yes** — this is the daily interaction. |
| Per-book link (`manual_mappings.ini`) | per new book | **Yes**, but only when auto-match fails. |
| `sync_scope`, `min_progress`, `finish_threshold`, `progress_step`, `match_threshold`, `poll_interval` | rarely | No — defaults + "advanced" screen. |
| Daemon start/stop, autostart | rarely | No — autostart after boot removes it. |
| Status: last sync, next sync, errors, per-book state | every glance | **Yes** — currently missing entirely. |

So the UI problem is 80 % **selection + status**, 20 % **one-time setup**.
That ratio drives the recommendation.

## 4. Options

### O0 — Keep KUAL, polish it (baseline)

Reorganise into `Status` / `Books` (paginated, 10 per page) / `Sync` /
`Advanced`, put counters in group labels (`"Books (12 of 34 synced)"`),
replace cde_keys with titles + percent, add a `[!]` marker for linked books,
and add one `Diagnose` action that writes its result into a menu entry
(`"Last sync: OK, 12 min ago"` / `"Last error: cookie expired"`).

- Gain: the current menu stops feeling like a log file; ~1 day of work.
- Ceiling: still no text input, still no login, still root-page resets, still
  no real status view. This cannot reach NickelStoryGraph's level.

### O1 — Move setup off the device (largest win per hour)

`kindlestorygraph setup` on the Mac (plus a `Cookie import` helper that reads
an exported browser cookie file), writing `config.ini` **directly into the
mounted Kindle** — or over SSH when USB networking is on (the reading-dashboard
project already does exactly this). Add `doctor` (cookie valid? cc.db
readable? daemon running?).

- Gain: kills the worst step (copying two cookies out of DevTools on a PC,
  then hand-editing an INI). No device UI risk at all.
- Cost: small, mostly CLI + one host script.
- Limitation: needs a cable, so cookie *renewal* still requires a PC unless O3
  exists. Mitigation: a long-lived cookie jar + `doctor` warning early.

### O2 — Native GTK control panel

A small GTK2 app (C, or Rust via the C API) that KUAL/scriptlet launches:
notebook with `Account`, `Books`, `Sync`, `Advanced`; a searchable list with
checkboxes; buttons for `Sync now`, `Link…`, `Unlink`; a status pane.

- Gain: closest to NickelStoryGraph's settings dialog; feels like the device;
  no extra runtime (GTK is already there); can be launched from a library
  scriptlet, i.e. no KUAL needed at all.
- Cost/risk: text entry needs its own keyboard (kterm-style) — for a password
  that is either painful or skipped (login then happens through O1/O3); GTK2
  API is ancient; e-ink ghosting/refresh has to be handled manually; shared-lib
  drift per model is a known failure mode (KUALA died on a missing `libbz2`);
  each firmware update can move the X/layer behaviour.
- Effort: medium–large. Verdict: the *right* UI, the *wrong* first step — and
  it duplicates work that O3 does in HTML.

### O3 — Local web panel in the stock browser (recommended target)

Give the binary a `serve` subcommand: bind `127.0.0.1:8099`, serve a small
static UI, JSON API over the existing config/state/allowlist functions. A KUAL
entry (or scriptlet) starts the server if needed and launches the browser at
that URL. `/usr/bin/browser` itself ignores arguments (it hardcodes the
Chromium flags), so the launcher either calls `kindle_browser` directly with
the same flags plus the URL, or extends the wrapper:

```sh
# must run as `framework`, not root (see the wrapper's own su)
su - framework -c "/usr/bin/chromium/kindle_browser --disable-gpu \
  --enable-low-end-device-mode --force-device-scale-factor=2 \
  --enable-grayscale-mode http://127.0.0.1:8099/"
```

Screens: `Status` (last/next sync, daemon, per-book table), `Books` (filter +
search + toggle, bulk "sync all shown"), `Link` (search box → candidates →
link), `Account` (email/password → **server** performs the StoryGraph login and
stores cookies; the browser never sees StoryGraph), `Advanced` (the numeric
settings), `Log` (last 200 lines).

- Gain: real text input (system keyboard), real scrolling, real dialogs,
  instant feedback, one codebase for device + Mac/phone (same page reachable
  over LAN), no native UI code, no GTK dependency, no root writes. This is the
  closest a non-hooked Kindle app can get to NickelStoryGraph.
- Cost/risk: (a) localhost must be reachable through the Kindle's proxy setup —
  **must be spiked**; (b) browser window lifecycle (back to library, resume
  after suspend) — needs a small experiment; (c) a second process to manage
  (start on demand, stop on idle); (d) the page must be e-ink friendly (high
  contrast, no animation, big hit targets); (e) Chromium is heavy — on a KT6
  it is the stock browser, so acceptable, but it is not free.
- Effort: medium. Reuses the whole existing CLI logic; the API is thin.

### O4 — WAF app (real Kindle app)

Register a folder `/opt/var/local/mesquite/kindlestorygraph/` (root write),
id `com.lab126.kindlestorygraph`, launch via `app://`. Gets Kindle chrome, its
own entry, and the system keyboard.

- Gain: looks most "built in".
- Cost/risk: root-owned path that an OTA can wipe; `appreg.db` registration
  needs on-device work; the WAF runtime is WebKitGTK-era (no modern JS); you
  still need the HTTP bridge to the Rust binary for the actual work. All the
  effort of O3 plus firmware fragility.
- Verdict: only worth it later, and only as a *shell* around the same O3 API.

### O5 — KOReader plugin façade

If KOReader is installed, a Lua plugin gives menus, input dialogs and a
keyboard for free and shells out to the binary.

- Gain: cheapest rich UI, if KOReader is already there.
- Cost: hard dependency on someone else's app; irrelevant for users who read in
  the native reader (our whole point).
- Verdict: optional side door, not the main path.

### O6 — Own framebuffer UI (FBInk, KUAL-Next style)

Draw everything yourself, implement a keyboard grid.

- Gain: no dependencies, single binary, full control.
- Cost: reimplementing widgets and text input; the result looks like KUAL Next.
- Verdict: no.

### Comparison

| Option | UX ceiling | Effort | Dependencies | Firmware risk |
|---|---|---|---|---|
| O0 menu polish | low | S | KUAL Next | low |
| O1 host setup | n/a (removes UI) | S | Mac + USB/SSH | none |
| O2 GTK panel | high | M–L | GTK2 + own keyboard | medium |
| O3 local web panel | high | M | stock Chromium | low–medium |
| O4 WAF app | high (most native look) | L | root, appreg, WebKit | high |
| O5 KOReader plugin | high | S | KOReader | low |

## 5. Recommendation

1. **v0.2 — O1 + O0.** Setup wizard/`doctor` on the host, and a re-grouped
   menu with a status line. Cheap, removes the worst friction immediately,
   and keeps the current UX fully working as fallback.
2. **v0.3 — O3.** `serve` + HTML panel; the KUAL entry becomes a launcher for
   it. Keep the KUAL menu as the "no browser / no wifi" fallback.
3. **Later, if it still matters — O4** as a thin shell over the same HTTP API;
   or O2 if a native-feeling panel turns out to be more important than a modern
   engine.

Architectural rule for all of it: **one JSON API, thin UIs.** `config.ini`,
`allowlist.ini`, `manual_mappings.ini`, `state.json` stay the storage format;
the loopback server is the only writer the UIs talk to, so KUAL actions, the
web panel and (later) a WAF or a host tool cannot drift apart.

## 6. Spike checklist before committing to O3 (~30 min on device)

1. `curl -s http://127.0.0.1:8099/` from KUAL (`wget`/`curl` on device) → does
   the loopback work at all (proxy/WRS not in the way)?
2. Launch Chromium at that URL from a KUAL action → does the page load, does
   the system keyboard appear on `<input>`?
3. Back button / swipe → returns to library cleanly? Browser process left
   behind (check `ps`)?
4. After suspend/resume, is the browser session still there?
5. Does the page survive e-ink refresh (no ghosting traps, `--enable-grayscale-mode`
   sufficient)?
6. Stop the server while the browser is open → clean error instead of a hang?
7. Same page from the Mac over USB networking → usable for setup without
   touching the device at all?

If (1) or (2) fails, fall back to O2 (GTK) and accept an own keyboard, or to
O1-only plus O0.

## 7. Decisions (answered 2026-10-19)

- **Plan:** start with O1 + O0; O3 later (on the roadmap); O6 optional, far
  later, only after everything works; O4 and O5 dropped (O4 overkill, O5 no).
- **Separate panel screen:** acceptable.
- **Auth:** cookie import only — a single break point when StoryGraph changes.
  No on-device password login.
- **Target floor:** also cover the 2022 Paperwhite (PW5/11th gen). It runs
  Chromium from 5.16.4 onward, so the floor is **5.16.4**; below that (WebKit
  browser) is out of scope unless the effort is justified later.
- **GTK2 (O2):** acceptable in principle, but lower priority since O3 is the
  target.
- **KUAL menu:** keep as a supported fallback.

## 8. Open questions — resolved (2026-10-19)

- **PW11 firmware:** 5.19.2 (last firmware Amazon shipped for it). ≥5.16.4, so
  it runs Chromium — O3 works there. Launcher not stated; 5.19.2 still runs
  classic KUAL, but the menu.json extension format is launcher-agnostic anyway.
- **Test access:** SSH (dropbear) available on the PW11; the Kindle Basic 2024
  (KT6) has no SSH yet. On-device spikes therefore run on the PW11 first; KT6
  testing is USB-only until usbnet is set up there.
- **Panel UI language:** multilingual (English + German), like NickelStoryGraph.
  Caveat: KUAL menu labels should stay ASCII — German umlauts may not render in
  KUAL Next's bundled Noto fonts, so the KUAL menu stays minimal-English while
  the CLI and web panel carry the EN/DE strings.
- **Cookie import:** Netscape-format `cookies.txt` (browser-agnostic) + manual
  paste fallback. Implemented as the `import-cookies` subcommand.

## 9. Implemented (2026-10-19, uncommitted)

- **O0 — menu polish:** new `Status` group (priority 0) showing last-sync age
  and synced count (or "Cookies not set" / "Not synced yet"); `Sync selection
  (N/M)` and `Link books (K)` headers carry counts; per-book entries show
  percent (`Dune  42%  [sync]`).
- **O1 — host setup:** `import-cookies <cookies.txt>` (parses Netscape cookies,
  writes `session_cookie` + `remember_user_token` into config.ini, preserving
  every other key) and `doctor [--cc-db <path>] [--network]` (health report).
  Both verified against a fixture `cc.db` on the host; `just lint` green
  (52 tests, clippy `-D warnings`).
