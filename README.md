<div align="center">

# KindleStoryGraph

**Sync your reading progress from the Kindle's own reader to The StoryGraph. No KOReader needed.**

[![Release](https://img.shields.io/github/v/release/dkuester/KindleStoryGraph?label=release)](https://github.com/dkuester/KindleStoryGraph/releases/latest)
[![CI](https://github.com/dkuester/KindleStoryGraph/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/dkuester/KindleStoryGraph/actions/workflows/ci.yml)
[![Kindle](https://img.shields.io/badge/Kindle-jailbroken%2C%20FW%205.17%2B-informational)](#requirements)
[![License](https://img.shields.io/github/license/dkuester/KindleStoryGraph)](LICENSE)

[Installation](docs/install.md) · [Features](#features) · [How it works](#how-it-works) · [Building](#building) · [Architecture](docs/architecture.md) · [Releases](https://github.com/dkuester/KindleStoryGraph/releases) · [Deutsch](README.de.md)

</div>

KindleStoryGraph syncs reading progress and reading status (currently reading
or finished) from a jailbroken Kindle to [The StoryGraph](https://thestorygraph.com),
while you keep reading in the Kindle's native reader. It is the Kindle
counterpart to [NickelStorygraph](https://github.com/dkuester/NickelStorygraph)
for Kobo and tolino, and uses the same StoryGraph login, book matching and sync
logic.

> [!NOTE]
> **Status:** reader, StoryGraph client, sync engine, KUAL packaging and
> cross-compile tooling are complete, and the sync is verified on-device
> (Kindle Basic 2024, firmware 5.20.1). Future work is in [ROADMAP.md](ROADMAP.md).

---

## Features

| Feature | Description |
|---|---|
| **Native reader** | Reads progress from the Kindle content catalog (`cc.db`). No KOReader, no other reading app. |
| **Manual or automatic** | "Sync now" in the KUAL menu runs a single pass. "Start sync daemon" checks every 15 minutes and pushes changes. |
| **Choose your books** | Opt-in allowlist by default: pick the books to sync in the KUAL menu. `sync_scope=all` syncs every book instead. |
| **No accidental starts** | Books below 2 % progress (the Kindle's first-open jump) are ignored until you really start reading. |
| **Manual matching** | Link a Kindle book to a StoryGraph edition from the KUAL menu ("Find matches" → "Link books") or in `manual_mappings.ini`. Linked books are marked `[linked]`. |
| **Remove from StoryGraph** | Changed your mind about a synced book? "Remove from StoryGraph..." in the KUAL menu opens the settings UI on the list of synced books; after a confirmation it (or `unsync <cde_key>`) resets it to "to-read" on StoryGraph and stops syncing it; "Remove completely" also deletes its reading history and review there. |
| **Light on the device** | Only progress *changes* are pushed, throttled per book. Polling is sparse, waits for Wi-Fi and respects suspend. |
| **Read-only** | Never writes to `cc.db` or your books. |

## Requirements

| | |
|---|---|
| Device | A **jailbroken** Kindle, e.g. via [Véra](https://kindlemodding.org/jailbreaking/Vera/) ([guide](docs/jailbreak.md)) |
| Firmware | about 5.17 or newer (developed and tested on 5.20.1, Kindle Basic 2024) |
| Launcher | KUAL Next or KUALA |
| Account | A StoryGraph account and Wi-Fi on the Kindle |

## Installation

> [!IMPORTANT]
> StoryGraph has no public API. KindleStoryGraph signs in with two session
> cookies from your browser. Treat them like passwords.

1. Download `kindlestorygraph-<version>-kual.zip` from the [latest release](https://github.com/dkuester/KindleStoryGraph/releases/latest).
2. Connect the Kindle over USB, create the folder `extensions/kindlestorygraph/`
   and unzip the contents into it.
3. Copy `config_example.ini` to `config.ini` and fill in `session_cookie` and
   `remember_user_token`.
4. In KUAL: **KindleStoryGraph → Sync now** or **Start sync daemon**.

The full guide, including how to get the cookies and all settings, is in
[docs/install.md](docs/install.md).

## How it works

Unlike on Kobo, the Kindle reader cannot be hooked in-process. KindleStoryGraph
is a small static binary that reads the Kindle content catalog (`cc.db`),
extracts progress and read state, matches each book to a StoryGraph edition and
pushes changes over the same HTTP layer NickelStorygraph uses. Details are in
[docs/architecture.md](docs/architecture.md).

## Building

Cross-compile the static armhf (musl) binary and assemble the KUAL extension
(Docker and [just](https://just.systems) required):

```bash
git clone https://github.com/dkuester/KindleStoryGraph.git
cd KindleStoryGraph
just build && just package
```

The extension is assembled in `packaging/extension/`. For development on the
host: `just lint` (fmt, clippy and the tests). `just emu-test` runs the armhf
binary and the KUAL scripts in a [virtual Kindle](docs/emulator.md).

CI runs fmt, clippy and the tests on every push to `main` and every pull
request. Every `v*` tag builds the KUAL zip in CI and attaches it to the
release.

## Support

| | |
|---|---|
| Bug reports and questions | [Issues](https://github.com/dkuester/KindleStoryGraph/issues). Please include Kindle model, firmware version and the relevant part of `kindlestorygraph.log`. **Remove cookies from logs and configs before posting.** |
| Downloads | [Releases](https://github.com/dkuester/KindleStoryGraph/releases) |
| Planned features | [ROADMAP.md](ROADMAP.md) |
| Earlier history | Pull requests up to v0.1.0 are in the [archived Codeberg repository](https://codeberg.org/hortizept/KindleStoryGraph). |
| Contributing | Pull requests against `main`; CI must pass. |

> [!WARNING]
> Jailbreaking can void your warranty. KindleStoryGraph is not affiliated with
> Amazon or The StoryGraph. Use it at your own risk.

## Acknowledgements

This project builds on prior reverse engineering and community work:

- [NickelStorygraph](https://github.com/VictoriaKerr131/NickelStorygraph) by
  VictoriaKerr131 (fork: [dkuester/NickelStorygraph](https://github.com/dkuester/NickelStorygraph)):
  the Kobo/tolino plugin whose StoryGraph login, book matching and sync logic
  are ported here.
- [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard):
  reverse-engineered and documented the `cc.db` schema (`p_percentFinished`,
  `p_readState`, …) and the modern read-state storage this project reads.
- [kaikozlov/kindle.koplugin](https://github.com/kaikozlov/kindle.koplugin):
  precedent for reading the Kindle content catalog from on-device code.
- [KindleModding](https://kindlemodding.org/): Véra jailbreak, Hotfix, KUAL
  Next, MRPI and KPM, the infrastructure this project depends on.
- The [KindleModding wiki](https://kindlemodding.org/kindle-hacking/) and the
  [MobileRead forums](https://www.mobileread.com/forums/): Kindle internals
  documentation.
- [KRDS](https://www.mobileread.com/forums/showthread.php?t=322172) by j.p.s /
  tomsem: parser for the `.sdr` sidecar files (referenced; used if finer
  position data is needed).
- [burneracc0112/storygraph.koplugin](https://github.com/burneracc0112/storygraph.koplugin):
  reference for StoryGraph sync patterns (KOReader).
- [sighery.com: Transferring books and progress between Kindles](https://sighery.com/posts/transferring-content-between-kindles/):
  `cc.db` / `Entries` documentation.

## AI assistance

This project is developed with AI assistance. The reverse engineering of the
Kindle reading-progress storage and the implementation are produced in
collaboration with AI tooling under human review. The underlying formats and
protocols were discovered by the community research credited above, not by AI.

## License

[MIT](LICENSE) © 2026 Daniel Küster
