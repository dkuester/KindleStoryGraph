# Notes on building Kindle mods (and talking to StoryGraph)

KindleStoryGraph is discontinued, but building it produced a lot of knowledge
that is hard to find in one place. These notes are meant to help the next
person who wants to write a mod for a jailbroken Kindle, or a StoryGraph
client.

**How far to trust this.** Everything here was checked while developing this
project, mostly on a Kindle Paperwhite (11th gen, firmware 5.19.2) and a
Kindle Basic 2024 (firmware 5.20.1). Some points come only from reading an
unpacked firmware image; those say so. Firmware, jailbreaks and StoryGraph's
website change, so treat version numbers as a snapshot from late 2026 and
re-check before relying on them.

Contents: [Architecture](#architecture) · [Jailbreak and launcher](#jailbreak-and-launcher) ·
[Reading data](#reading-data) · [KUAL extensions](#kual-extensions) ·
[Drawing a UI](#drawing-a-ui) · [Files on `/mnt/us`](#files-on-mntus) ·
[Cross-compiling and testing](#cross-compiling-and-testing) ·
[Reading a firmware image](#reading-a-firmware-image) ·
[StoryGraph's web endpoints](#storygraphs-web-endpoints) · [Pitfalls](#pitfalls)

## Architecture

There is no equivalent of NickelHook on the Kindle. Kobo's reader (Nickel) is a
Qt application whose `libnickel.so` can be hooked in-process. The Kindle UI is a
WebKit/HTML5 "booklet" framework plus Java services under `/opt/amazon/ebook/`,
and there is no scene for patching the reader binary.

Consequence: a mod that has to react to what the native reader does is a
**daemon or poller** (an upstart service, or a KUAL extension that reads
on-disk state), never an in-process hook. A Kobo plugin such as
[NickelStorygraph](https://github.com/dkuester/NickelStorygraph) cannot be
ported one to one. Only its network layer ports cleanly. The part that gets
reading progress has to be rewritten around polling.

## Jailbreak and launcher

Mods need a jailbreak. Methods are tied to firmware and model, so use the
[kindlemodding.org](https://kindlemodding.org/) jailbreak wizard instead of a
fixed table. As of late 2026:

| Method | Firmware |
|---|---|
| WinterBreak | below 5.18.1 |
| AdBreak | 5.18.1 to 5.18.5.x |
| Nosebleed | 5.18.6 |
| SpringBreak | Kindle Basic 2022 / Paperwhite 11th gen on 5.19.2.x; Basic 2019 / Paperwhite 10th gen on 5.18.1.1.1 |
| Véra | Basic 2022 and 2024, Paperwhite 11th and 12th gen, Colorsoft, Scribe (published for 5.17.1 to 5.19.6) |

- Published ranges are conservative, not hard limits. Véra is listed up to
  5.19.6 and works on 5.20.1; its guide says to try the nearest listed
  firmware. A first-hand test on your model beats the published range.
- Newer methods install the hotfix and block OTA updates automatically.
- Each exploit gets patched, so a new major firmware can need a new jailbreak.
  The data formats (see below) stay stable across firmware, the jailbreak does
  not.

**Launcher.** Classic KUAL (the Java booklet) stops working on firmware 5.19.3
and newer because Amazon removed the Java API it needed. On 5.19.4 and newer
(including 5.20.1) use **KUAL Next** (`thiagokokada/kual-next`, native, uses
FBInk) or **KUALA** (`kasparcode/kuala`, GTK). Both read the same `config.xml`
and `menu.json` extension format, so extensions do not change, only the
launcher does. KUAL Next is armhf only (Basic 2022 and newer, Paperwhite 11th
gen and newer, Colorsoft); older armel devices keep classic KUAL.

## Reading data

There are two sources for reading progress.

**`/var/local/cc.db`**, the SQLite content catalog and the source of the "% read"
badge. It needs root: it is not visible over USB/MTP, so you need SSH,
USBNetwork or a terminal on the device, or a dump from the user. The reading
framework holds it open, so open it **read-only** and tolerate WAL/locking
instead of assuming a clean snapshot. Table `Entries`:

| Column | Meaning | Notes |
|---|---|---|
| `p_cdeKey` | content key (ASIN or UUID) | identity key |
| `p_cdeType` | `EBOK` (store book) or `PDOC` (personal document) | sideloaded and Send-to-Kindle books are `PDOC`, so filtering to `EBOK` drops them |
| `p_titles_0_nominal` | display title | often has long subtitles |
| `j_credits` | authors | JSON string |
| `p_percentFinished` | percent read | float, 0 to 100 |
| `p_lastAccess` | last opened | epoch seconds |
| `p_readState` | read state | unreliable, do not use |
| `p_isArchived` | 1 = cloud-only placeholder | filter to 0 |

Rules that took some effort to find:

- "Finished" means `p_percentFinished >= 99.5`, not `p_readState` and not
  `== 100`. On firmware 5.20.1 `p_readState` takes values outside the
  documented 0/1/2 (3, 4 and 6 were observed), and a stored 100.0 can be a
  rounded 99.6.
- Use `WHERE p_isArchived = 0`: archived rows have no percent or location.
  Deduplicate by `p_cdeKey` anyway, keeping the greatest `p_lastAccess`.
- "In progress" is `p_percentFinished > 0` and below the finish threshold.
- A book jumps to about 1 % the moment it is opened. A sync that attaches
  everything above 0 % pollutes the target account with books the user only
  peeked at, so use a minimum (this project uses 2 %).
- The `Entries` schema was identical between firmware 5.18.1 and 5.20.1.

**`.sdr` sidecar folders** under `/mnt/us/documents/<book>.sdr/`, with
proprietary per-format files (KF8 `.azw3f`/`.azw3r`, KFX `.yjf`/`.yjr`, MOBI
`.mbp1`, PDF `.pdt`/`.pds`), decoded by the KRDS parser from MobileRead. They
are readable over a plain USB mount (no root) and carry finer data such as the
exact position and reading time, but the format differs per book type and KRDS
is incomplete. This project did not use them.

## KUAL extensions

An extension is a folder `/mnt/us/extensions/<name>/` with `config.xml`,
`menu.json` and scripts in `bin/`. Copying it over USB is enough; no MRPI or
KPM package is needed for the user partition.

- `config.xml`: `<extension><information>…</information><menus><menu type="json" dynamic="true">menu.json</menu></menus></extension>`
- `menu.json`: `{"items":[{"name":"Submenu","priority":0,"items":[{"name":"Label","priority":1,"action":"bin/x.sh","exitmenu":true}]}]}`.
  `action` is relative to the extension folder.
- KUAL runs actions as root, so they can read `cc.db` directly.
- Start long-running daemons with `nohup … &` so the launcher returns at once,
  and stop them with `pkill -f "<binary> daemon"`. Match the daemon's arguments:
  a broad pattern like `bin/<binary>` also kills a running toggle action and,
  when the script is started by full path, the wrapper shell itself.
- The binary sits in `bin/`, one level below the config, so pass the config
  path through an environment variable set in the wrapper script.

### The KUAL Next menu contract

Read out of KUAL Next's source (`src/menu.c`, `src/ui_fbink.c`). Getting these
wrong makes an extension look broken instead of misconfigured.

- **`exitmenu` defaults to true.** An entry without `"exitmenu": false` makes
  the launcher replace itself with the action. The script finishes, but the
  menu is gone ("KUAL closes whenever I tap a book").
- **`refresh` does nothing on an exiting entry.** The refresh branch comes
  after the early return for exiting entries, so `"refresh": true` needs
  `"exitmenu": false` next to it.
- **The refresh window is about 250 ms.** The launcher forks the action, waits
  250 ms and re-reads `menu.json`. An action that starts a shell and two
  binaries loses that race, and the stale menu then stays until the launcher
  restarts. Do the work **and** rewrite `menu.json` in one process (`exec` the
  binary from the wrapper) and time it on the host.
- **A reload always returns to the root page, page 1.** After toggling a book
  the user lands back at the top, so plan for one extra tap.
- **`checked` is press-only.** It means "draw a check after this is pressed",
  not initial state, so persisted state cannot live there.
- `params` is appended to the action command and split on spaces into `$1`,
  `$2`. Verify multiple arguments on the device.
- KUAL Next loads menus at startup and after `refresh` entries only. There is no
  live update protocol. Regenerate the file and let the launcher re-read it.

Patterns that follow from this:

- **Encode state in the label**: `"Title  [sync]  [linked]"`, plain ASCII.
- **There is no passive label.** Every leaf needs an action. For a status
  line, emit a top-level group whose single child carries the text plus a
  harmless `refresh` action. Counts can sit in a group header
  (`"Sync selection (2/12)"`).
- **Never overwrite a good menu with an empty one.** If `cc.db` is missing or
  locked, bail out and keep the old `menu.json`. Write through a temp file and
  rename so the launcher never reads a half-written file.
- **Pick-lists from a search: cache, do not search live.** Searching while
  generating the menu runs on the device with network and cookies and hangs
  every refresh. Search once in a separate action, cache candidates in a JSON
  file, and render a nested submenu from the cache.
- **The menu cannot take input, show dialogs or confirm anything.** A tap runs
  its action immediately. Anything destructive or needing text input belongs in
  a real UI (see below) or on the host.

## Drawing a UI

When the menu is not enough, you have these options on firmware 5.19 and 5.20.

**Browser panel (a dead end here).** The idea is to serve a single-page UI from
the binary on `127.0.0.1` and open it in the device browser. It does not work
on 5.19.2 and 5.20.1:

- The user-facing browser is the `com.lab126.browser` WebKit app, not Chromium.
  It rejects loopback URLs ("invalid protocol") while external `https://` URLs
  load fine.
- The old Chromium engine (`kindle_browser`) has no `file://` support, runs
  differently per firmware (root in a chroot on 5.16 to 5.19; user `framework`
  on 5.20), needs the wrapper's environment (`LD_LIBRARY_PATH`) to start at all,
  and ships without swiftshader on 5.19.x and 5.20.x. A standalone launch hangs
  without drawing a window. On 5.16 to 5.19 the chroot also lacks
  `/tmp/.X11-unix`, so the X socket is invisible inside it.
- If you try anyway: bind `127.0.0.1` only (the server holds the session
  cookies), and diagnose over SSH. A failed launch from KUAL just leaves a
  frozen-looking screen.

**FBInk (what this project uses).** Draw straight to the e-ink framebuffer with
[FBInk](https://github.com/NiLuJe/FBInk) and read touch events from
`/dev/input/event*`. No X, no browser, no toolkit. KUAL Next itself is built on
FBInk, so the approach is proven on the device.

- Statically link FBInk into the (Rust) binary from `build.rs` with the `cc`
  crate. Vendor `fbink.c`, `fbink.h`, `fbink_internal.h`, `cutef8/`,
  `font8x8/`, `eink/` and `fonts/`, and compile `fbink.c` plus
  `cutef8/{utf8,dfa}.c` with `FBINK_FOR_KINDLE`, `FBINK_MINIMAL`,
  `FBINK_WITH_DRAW`, `FBINK_WITH_BITMAP`, `FBINK_WITH_FONTS` and
  `FBINK_WITH_INPUT`. Leaving out OpenType, image and Unifont support avoids
  extra dependencies and keeps the binary small. The `fonts/` folder is needed
  (the font headers include `fonts/*.h`); skip `unifont.h` and `unifontdw.h`
  (about 8 MB), which are only included with Unifont support. Skip FBInk
  entirely for non-Linux targets so host builds stay fast. The `messense`
  cross image names its compiler `armv7-unknown-linux-musleabihf-gcc`; set it
  explicitly in `cc::Build`.
- **Check FFI struct layouts empirically.** The structs depend on `#ifdef`
  flags. Compile a throwaway C file with the same flags that returns
  `sizeof`/`offsetof`, and read the numbers from the assembly. On armv7 with
  these flags: `FBInkConfig` is 42 bytes, `FBInkRect` 8, `FBInkInputDevice`
  4364 and `FBInkState` 180. Mirror `FBInkState` at full size: `fbink_get_state`
  writes the whole struct, so a truncated Rust copy overflows the stack.
- **Map touch coordinates before hit-testing.** `FBInkState` carries panel
  quirks: `touch_swap_axes`, `touch_mirror_x`, `touch_mirror_y`. Swap x and y
  first, then mirror each axis against the screen size, then subtract
  `view_vert_origin` before dividing `y` by the row height. Without this,
  taps land on the wrong row on some panels, which only shows on the device.
- **Never block the tap loop on the network.** Make the input fd non-blocking
  (`fcntl` with `O_NONBLOCK`), run the request on a thread and poll an `mpsc`
  channel each iteration, sleeping about 15 ms on `EAGAIN`. Draw the
  "searching…" state before spawning the thread, keep a cancel zone, and give
  the request its own timeout, because a global client has none.

Text input is the weak spot of this approach. The stock on-screen keyboard is
a separate X client (`kb`, started by upstart) that only the native UI gets.
This project avoided text input by importing cookies on the host.

## Files on `/mnt/us`

The user partition is vfat and every file the mod owns is touched by two
independent processes: the daemon and a KUAL action (which runs as the
launcher process).

- **Write atomically**: temp file in the same folder, `sync_all()`, then
  `rename()`. Also try to fsync the parent folder, but ignore the error:
  opening a directory for fsync fails on vfat. A power cut or suspend
  otherwise leaves a truncated file.
- **Self-heal corrupt files**: rename to `<file>.corrupt-<epoch>`, log it and
  continue with empty state. A load error that repeats every cycle disables
  the mod for good and the user only sees "sync error".
- **Serialise the whole read-modify-write cycle** with
  `flock(LOCK_EX | LOCK_NB)` on `<state>.lock`. Atomic writes do not stop two
  passes from overwriting each other. Retry briefly (5 × 250 ms), then fail
  with a clear message instead of blocking, because KUAL runs actions as the
  launcher process and a blocked action freezes the device with nothing on
  screen. The kernel drops the lock when the process dies, which is why this
  beats a PID file.
- Report expected failures (lock contention, expired cookies) as a typed
  user-facing error. A generic "unexpected error, please report this" reads as
  a bug.

## Cross-compiling and testing

Target `armv7-unknown-linux-musleabihf` (static musl), not
`…-gnueabihf`. The Kindle's glibc is old, and a glibc binary linked on a modern
toolchain aborts with `GLIBC_x.yy not found`. A static musl binary has no
version to mismatch. Build in Docker so the host needs no ARM toolchain:

```sh
docker run --rm -v "$PWD":/app -w /app \
  -v ksg-cargo-registry:/root/.cargo/registry \
  messense/rust-musl-cross:armv7-musleabihf \
  cargo build --release --target armv7-unknown-linux-musleabihf \
  --target-dir /app/target-armv7
```

`file` on the result should say `ELF 32-bit LSB executable, ARM, EABI5,
statically linked`. Bundled C dependencies (rusqlite's SQLite, FBInk) build
fine because the image ships a musl gcc. CI uses the same image. A
Docker-free route with `cargo zigbuild` was planned but never tried here.

Without a device you can still test a lot:

- Run the CLI and menu generator on the host against a fixture `cc.db` and a
  temp config, with **every** `*_path` key pointing at scratch files. A leftover
  `cc_db_path=/var/local/cc.db` makes the snapshot silently empty.
- Run the real armv7 binary and the real KUAL scripts in an emulated ARM
  container with the device paths mounted, and a fake StoryGraph for end-to-end
  runs. See [emulator.md](emulator.md) for what this covers and what stays a
  device test (touch, e-ink, KUAL Next itself, `cc.db` locking, suspend/resume,
  vfat).
- A static armhf binary cannot run on the host (it fails with an unhelpful
  `Undefined error: 0`).
- To iterate over SSH instead of replugging USB: `scp` does not create missing
  folders and cannot overwrite a running binary (`ETXTBSY` on vfat), and both
  show up as the same `scp: dest open …: Failure`. Before each deploy: create
  the folder, stop the running instance, delete the old binary, copy, `chmod
  +x`. Deploy the whole extension, not just the binary.
- To check that a label or menu change made it into a build, run the build
  against a fixture and read the JSON. Grepping the binary for short strings
  gives false negatives because linkers merge string constants.

## Reading a firmware image

You can unpack a current firmware on a Mac in about 15 minutes, which beats
guessing whether a library or API exists. Verified on
`update_kindle_11th_2024_5.20.1.bin` (Basic 2024):

- Download from `https://s3.amazonaws.com/firmwaredownloads/<filename>.bin`.
- Use KindleTool 1.6.6 (`NiLuJe/KindleTool`). Older forks fail with `Unknown
  update bundle version FB03`. Building it on macOS needs `brew install
  libarchive nettle zlib` and a small patch for nettle 4: `md5_digest` and
  `sha256_digest` no longer take a length argument.
- `kindletool convert -c -k x.bin > x.tar.gz`, then `tar xzf` gives
  `rootfs.img.gz`. `gunzip` it to get an ext3 image, then dump it with
  `debugfs -R "rdump / <outdir>" rootfs.img` (`brew install e2fsprogs`).
  Per-file "Operation not permitted while changing ownership" messages only mean
  the chown failed; the content is complete.
- `/usr/bin/chromium/` is empty in a dump because it is a squashfs
  (`chromium.sqsh`) mounted at boot. List it with `7z l chromium.sqsh`.

What this showed on 5.20.1 (from the image, not all tried on a device):

- The UI is X11 with the awesome window manager and Lua layers in
  `/etc/xdg/awesome/lab126_*_layer.lua`.
- GTK+ 2.20.1 is installed, no GTK3, so C/C++ GTK2 apps are possible (KUALA,
  kterm and Abiword are precedents).
- The on-screen keyboard is the `kb` client driven over LIPC. Foreign GTK apps
  do not get it; kterm and Abiword ship their own.
- Web apps are `config.xml` folders under `/opt/var/local/mesquite/`,
  registered in `/var/local/appreg.db` and started with
  `lipc-set-prop com.lab126.appmgrd start app://<id>`.
- USB/MTP drive mode is `com.lab126.volumd`. There is no clean way to
  re-enter it from the device; re-enumeration needs a physical replug.

## StoryGraph's web endpoints

StoryGraph has no public API. This project does what a browser does: session
cookies, a CSRF token, HTML scraping for reads and form posts for writes. None
of this is documented or stable, and the project's own tests use a fake server
whose pages mirror the parsers, not captured pages.

**Authentication.** The cookies `_storygraph_session` and `remember_user_token`
(copied from the browser's developer tools; the CLI can import a Netscape
`cookies.txt`). The popup cookies `cookies_popup_seen=yes` and
`plus_popup_seen=yes` avoid interstitials. An expired session answers with a
redirect to `/users/sign_in`.

The sign-in page itself sits behind a Cloudflare bot challenge (HTTP 403, "Just
a moment…") for non-browser clients. Scripted login therefore does not work
reliably, which is why the tool relies on cookies from a real browser session.
That is also the main reason this project was stopped.

**Writes** need the CSRF token from the book page (`<meta name="csrf-token">`
or the `authenticity_token` input), sent both as a form field and as the
`X-CSRF-Token` header, together with `X-Requested-With: XMLHttpRequest`,
`Origin` and `Referer`. A `302` back to the book page means success.

| Endpoint | Purpose |
|---|---|
| `GET /browse?search_term=<q>` | search; results are `.book-title-author-and-series` blocks with `/books/<id>` and `/authors/…` links; audio editions say "Format: audio" |
| `GET /books/<id>` | book page: status in `.read-status-label`, progress in the `edit-progress` bar's `width` style, hidden `read_status[…]` inputs for pages |
| `POST /update-status.js?book_id=<id>&status=<slug>` | set status: `to-read`, `currently-reading`, `read`, `paused`, `did-not-finish`, `rereading` (ids 1 to 6 in that order) |
| `POST /update-progress` | form fields `read_status[progress_number]`, `read_status[progress_type]` (`percentage` or `pages`), `read_status[book_num_of_pages]`, `book_id`, `on_book_page=true` |
| `GET /books/<id>/check_tags` | JSON with `has_tags`, used before removing a book |
| `POST /remove-book/<id>?remove_tags=<bool>` | remove the book from all lists |

Behaviour worth knowing:

- Pushing progress for a book that is already **read** makes it a reread: this
  client switches such a book to **rereading** before posting the progress.
  When a book is finished, set the status only and do not push a final 100 %.
- Setting **to-read** only resets the status. The book stays on the to-read
  list and the reading journal keeps its history ("Started reading" and the
  progress entries). Verified on a test book in October 2026.
- **Remove book** leaves no status, progress or journal entry behind, and also
  deletes any review. Tags can be kept. Verified the same way.
- Progress made elsewhere should win. Read the current value before pushing and
  skip the push when StoryGraph is ahead or the book is already read, so a
  sync never moves a book backwards.
- Edition and language matter. A sync tool that attaches books on its own must
  support a dry run and a manual override, because title matching is lossy.

**Matching Kindle books to StoryGraph books.** Score titles as the F1 of word
recall and precision over unique word sets, not recall alone: recall lets a
box set ("… 8-Book Collection Set (Book 1-8)") outrank the single book.
Normalise the query first: strip a trailing parenthetical like "(German
Edition)", and cut a colon subtitle only when it is a long blurb (more than 4
words), because short subtitles tell editions and languages apart. Use the
author only as a tie-breaker between same-title editions, never as a veto.

## Pitfalls

- Default to **opt-in** when pushing private reading history to a public
  profile: a per-book allowlist, with "sync everything" as a switch. A new book
  stays silent until chosen. Removing a book from the allowlist does not remove
  it from the profile, so offer an explicit removal as well.
- Resolve the target book as manual mapping, then persisted state, then title
  search, and re-read the mapping file every cycle. A wrong match is then fixed
  by adding one line, with no state deletion and no restart.
- Isolate failures per book: one failing request must not abort the pass or
  throw away the state of the books already synced.
- Do not let expected conditions look like bugs: expired cookies, a locked
  state file, a missing `cc.db` or framebuffer should all give a clear message.
