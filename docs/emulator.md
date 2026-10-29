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
  the Kindle) run through QEMU by Docker, with no network.
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

The suite takes a few seconds after the build and needs no network:

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
