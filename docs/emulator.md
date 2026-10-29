# Virtual Kindle (emulator)

A full Kindle emulator is not possible (proprietary hardware and firmware), but
most device-side breakage is not in the firmware: it is in the wrapper
scripts, paths, permissions, the static binary and the generated menu. The
virtual Kindle runs exactly those parts on your computer, so they fail there
first. It supports the device tests; it does not replace them.

## What it is

- The **real** static armv7-musl binary from `just build`
  (`target-armv7/armv7-unknown-linux-musleabihf/release/kindlestorygraph`).
- The **real** KUAL wrapper scripts from `packaging/extension/bin/`, unchanged.
- An ARM Linux container (`alpine:3.20.3`, `linux/arm/v7`, busybox `sh` like
  the Kindle) run through QEMU by Docker, with no outside network: in
  `emu-test` it shares only a loopback interface with the fake StoryGraph
  (see [Stage 2](#stage-2-fake-storygraph)).
- A scratch tree under `target/emu/run/` mounted at the device paths, so no
  script needs rewriting:

| Container path | Contents |
|---|---|
| `/mnt/us/extensions/kindlestorygraph` | the extension as `just package` lays it out (tracked files with their git modes, the binary, the example configs) plus `tests/emu/emu_config.ini` as `config.ini` (every `*_path` on the device paths, no cookies) |
| `/var/local/cc.db` | built from `tests/emu/cc_fixture.sql` on every run |

The repository is never mounted into those paths; every run starts from a
fresh copy.

## Usage

Docker (with `linux/arm/v7` emulation — Docker Desktop has it built in; on
Linux install QEMU binfmt, e.g. `docker run --privileged --rm
tonistiigi/binfmt --install arm`), `just` and `python3` are required.

```bash
just build
just emu-test    # scripted smoke suite, exits non-zero on failure
just emu-shell   # interactive shell in the emulated Kindle
```

In the shell you are in the extension dir, like KUAL. Run actions the way the
launcher does, e.g. `sh -c "bin/toggle-book.sh B00DUNE001"`.

- `KSG_EMU_CC_DB=/path/to/cc.db` uses a private cc.db dump instead of the
  fixture (`emu-test` then skips the fixture-specific checks). Never commit a
  real cc.db.
- `KSG_EMU_NETWORK=1` gives `emu-shell` network access (off by default; no
  cookies are configured either).

## What `emu-test` covers

The suite takes a few seconds after the build and needs no network (beyond
pulling the `alpine` and `python` images once):

- packaging: every `bin/*` executable, `#!/bin/sh` shebangs, `config.xml`
  pointing at `menu.json`, the packaged `menu.json` valid JSON
- the binary starts under ARM (`--help`), `doctor` (without `--network`)
  reads the config and the cc.db
- `bin/refresh-menu.sh` writes a valid `menu.json` (validated on the host)
  with the expected books: archived duplicates, books below `min_progress`,
  the jailbreak dummy and collection rows hidden; finished, same-title and
  non-ASCII books listed; no "Remove from StoryGraph..." until a book is in
  `state.json`, then "Remove from StoryGraph... (1)"
- `bin/toggle-book.sh <key>` updates `allowlist.ini` and `menu.json`
  ("Sync selection (N/M)", `[sync]`); its wall time is reported against KUAL
  Next's ~250 ms refresh window. That number is informational only: QEMU on
  the host CPU does not predict the Kindle's Cortex-A7
- `bin/link-book.sh` writes `manual_mappings.ini` and marks the book
  `[linked]`
- a missing cc.db leaves the existing `menu.json` untouched
- `bin/rotate-log.sh`, and the start-time rotation in `bin/start.sh` with an
  artificially grown log; `start.sh`/`stop.sh` start and stop the daemon
- `sync --dry-run` and `bin/sync-once.sh` without cookies fail with the clean
  "set session_cookie" message (no "unexpected error"), and write no state
- `ui` without a framebuffer exits with a clean error instead of crashing
- one real sync through `bin/sync-once.sh` against the fake StoryGraph: two
  allowlisted books attached (the same-title book to the right author's
  edition), the shelf checked on the server side, `state.json` written; a
  non-loopback `KSG_STORYGRAPH_BASE_URL` refused

CI runs the same suite in the `emu-test` job.

## What stays a device test

- touch input, e-ink drawing and refresh (the FBInk UI); the emulator has no
  framebuffer
- KUAL Next itself: menu rendering, `exitmenu`/`refresh` behaviour and the
  real ~250 ms refresh race
- cc.db while the framework holds it open (locking, WAL)
- suspend/resume, Wi-Fi coming and going, the daemon over hours
- vfat on `/mnt/us`: no Unix permissions (everything is executable there),
  rename and fsync semantics; the emulator's scratch tree is a normal
  filesystem
- the real StoryGraph session (cookies, Cloudflare, network)

## Stage 2: fake StoryGraph

The sync engine's unit tests (`src/sync.rs`) use a mock of the client trait,
so they never touch the HTTP client, the HTML parsers or the CLI. Stage 2
runs the **real binary** against a local **fake StoryGraph**, so full sync,
remove and error flows are tested without a StoryGraph account.

### Pieces

- `tests/e2e/fake_storygraph.py` — Python stdlib only, stateful and
  deterministic. Python rather than a Rust helper because it runs unchanged
  on the host (`cargo test`) and in a stock `python` container next to the
  emulator, keeps the stateful fake short, and needs no extra crates. It
  serves `GET /`, `/browse?search_term=`, `/books/<id>`,
  `/books/<id>/check_tags`, `POST /update-status.js`, `/update-progress`,
  `/remove-book/<id>` and the sign-in page/form, and it **enforces** what the
  real site does: a valid `_storygraph_session` (rotated on every response)
  or `remember_user_token`, else a redirect to `/users/sign_in`; the CSRF
  token and a matching `Origin` on every POST, else 422.
- `tests/e2e/templates/*.html` and `tests/e2e/catalog.json` — the pages and
  the catalog (same-title editions are listed wrong-one-first on purpose, an
  audio edition and "Dune Messiah" sit next to "Dune").
- A control channel that is not part of StoryGraph: `GET /__state` (shelf,
  tags, every mutation as an event, the request log), `POST /__reset`,
  `POST /__fault` (500, slow response, expired session, wrong-format page;
  per path prefix, optionally limited to N hits), `POST /__clear_faults`.
- `tests/e2e.rs` — Rust integration tests: each test starts its own fake on
  port 0 and its own temp dir (config with every `*_path` in it, fake
  cookies, a cc.db built from `tests/emu/cc_fixture.sql`), runs the binary
  (`CARGO_BIN_EXE_kindlestorygraph`) and asserts on **both** the CLI outcome
  (JSON, exit code, messages) and the server state.

### Pointing the binary at the fake

`KSG_STORYGRAPH_BASE_URL` overrides `https://app.thestorygraph.com`. Only
`http://127.0.0.1:<port>` and `http://localhost:<port>` are accepted (no
path, credentials, query); anything else is refused with an error instead of
falling back, because the session cookies go to whatever host it names.
Unset, the behaviour is unchanged. `KSG_STORYGRAPH_SEARCH_TIMEOUT_MS`
shortens the 15 s search timeout, and is honoured only together with the
override.

### Running

```bash
just e2e-test          # = cargo test --test e2e (also part of just lint)
just fake-storygraph   # the fake on 127.0.0.1:8080 for manual tries
```

e.g. `KSG_STORYGRAPH_BASE_URL=http://127.0.0.1:8080 cargo run -- search Dune`
with a config that has `session_cookie=fake-session-cookie`. CI runs the e2e
tests in the `test` job and the emulator scenario in `emu-test`.

### What the e2e tests cover

- first sync: books attached, status and progress set, unknown books
  `no_match`; same-title editions resolved by author, the audio edition and
  a superset title ("Dune Messiah") not picked
- progress updates, and no write below `progress_step`
- the backward-progress guard: `skipped_behind` when StoryGraph is further
  along or the book is already read there, nothing written
- finishing: status "read" only, never "rereading", nothing pushed later
- per-book failure isolation: one book's 500 fails only that book, the
  others sync and are saved, exit code non-zero; the next pass retries it
- expired session: `sync`, `unsync`, `whoami`, `search` all end with the
  "session has expired" message (never "unexpected error"), nothing changed
- `unsync` → "to-read", allowlist and state updated, next pass leaves it
  alone; `unsync --completely` → gone server-side, tags kept only when the
  book has any; a never-synced book refused
- `--dry-run`: no POST at all, server and state unchanged
- search timeout and wrong-format pages surface as clean per-book failures
- `whoami` and `login` (cookie round trip), and the base-URL guard

### Drift: the fake is not StoryGraph

The fake's HTML is written to match the parsers in `src/storygraph.rs`, not
captured from StoryGraph. A green run proves the client and the fake agree;
it cannot notice StoryGraph changing its markup or endpoints. Known blind
spots:

- a search page without the expected markup is indistinguishable from "no
  results": the book becomes `no_match`, silently (a test pins this down)
- the audio filter looks for "Format: audio" in a search result; whether the
  live search page still prints that is unverified
- Cloudflare, rate limits and real cookie lifetimes do not exist here

So the live-account checks stay in the device checklist: one real sync, one
`unsync` and a look at the StoryGraph shelf after each release. When a live
check finds new markup, change the parser, the templates and their test
together.
