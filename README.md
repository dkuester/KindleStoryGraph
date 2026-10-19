# KindleStoryGraph

StoryGraph sync for the native Kindle reader — the Kindle counterpart to
[NickelStoryGraph](https://github.com/dkuester/NickelStorygraph) (Kobo/Tolino).

Synchronizes your reading progress and reading status (currently reading /
finished) from a jailbroken Kindle to
[The StoryGraph](https://thestorygraph.com), without switching reader apps.

> **Status:** Phases 1–4 complete. Reader, StoryGraph client, sync engine, KUAL
> packaging and cross-compile tooling are done and the sync is verified on-device.
> Also includes opt-in allowlist sync, a minimum-progress threshold, manual
> title→book mapping, and a manual-only mode. See
> [docs/architecture.md](docs/architecture.md).

*[Deutsche Übersetzung](README.de.md)*

## How it works

Unlike Kobo, the Kindle reader cannot be hooked in-process. KindleStoryGraph
reads the Kindle content catalog (`cc.db`), extracts reading progress, and
pushes it to StoryGraph over the same HTTP layer NickelStoryGraph uses.

Run it two ways:

- **Manual** — "Sync now" in the KUAL menu runs a single pass and exits; nothing
  runs in the background.
- **Daemon** — "Start sync daemon" polls `cc.db` every 15 minutes and pushes
  changes automatically.

- Native reader only — no KOReader required.
- Read-only: never touches `cc.db` or your books.
- Sparse polling, Wi-Fi gated, suspend-aware.

## Features

- **Opt-in allowlist** (default): choose which books sync from the KUAL menu;
  `sync_scope=all` in `config.ini` syncs every book instead.
- **Minimum-progress threshold**: books below 2 % (the Kindle's first-open jump)
  are ignored until you really start reading.
- **Manual mapping**: pin `cde_key → StoryGraph id` for titles the search can't
  match — either from the KUAL menu ("Find matches" → "Link books") or by
  editing `manual_mappings.ini`.
- **Link status in the menu**: books already linked to StoryGraph show a
  `[linked]` marker in "Sync selection".
- **Diff-based sync**: only progress *changes* are pushed, throttled per book —
  never a full-library re-upload.

## Requirements

- A jailbroken Kindle (e.g. [Véra](https://kindlemodding.org/jailbreaking/Vera/)).
- Firmware ~5.17+ (developed and tested on 5.20.1, Kindle Basic 2024).

## Install

See [docs/install.md](docs/install.md) for the full guide; the jailbreak
prerequisite is documented in [docs/jailbreak.md](docs/jailbreak.md).

## Building

Cross-compile the static armhf (musl) binary and assemble the KUAL extension
(Docker required):

    just build && just package

The extension is assembled into `packaging/extension/`; copy that folder to
`/mnt/us/extensions/kindlestorygraph` on the Kindle. Tagged `v*` releases build
a zip via CI (`.forgejo/workflows/release.yml`).

## Project status

Phases 1–4 complete and the sync is verified on-device. See
[docs/architecture.md](docs/architecture.md) for the full plan and
implementation phases. Future work is tracked in [ROADMAP.md](ROADMAP.md).

## Credits

This project builds on prior reverse engineering and community work:

- [NickelStoryGraph](https://github.com/VictoriaKerr131/NickelStorygraph) by
  VictoriaKerr131 (fork: [dkuester/NickelStorygraph](https://github.com/dkuester/NickelStorygraph)) —
  the Kobo/Tolino plugin whose StoryGraph login, book-matching and sync logic
  are ported here.
- [zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard) —
  reverse-engineered and documented the `cc.db` schema (`p_percentFinished`,
  `p_readState`, …) and the modern read-state storage that this project reads.
- [kaikozlov/kindle.koplugin](https://github.com/kaikozlov/kindle.koplugin) —
  precedent for reading the Kindle content catalog from on-device code.
- [KindleModding](https://kindlemodding.org/) — Véra jailbreak, Hotfix, KUAL
  Next, MRPI and KPM, the infrastructure this project depends on.
- The [KindleModding wiki](https://kindlemodding.org/kindle-hacking/) and the
  [MobileRead forums](https://www.mobileread.com/forums/) — Kindle internals
  documentation.
- [KRDS](https://www.mobileread.com/forums/showthread.php?t=322172) by j.p.s /
  tomsem — parser for the `.sdr` sidecar files (referenced; used if finer
  position data is needed).
- [burneracc0112/storygraph.koplugin](https://github.com/burneracc0112/storygraph.koplugin) —
  reference for StoryGraph sync patterns (KOReader).
- [sighery.com — Transferring books and progress between Kindles](https://sighery.com/posts/transferring-content-between-kindles/) —
  `cc.db` / `Entries` documentation.

## AI assistance disclosure

This project is developed with AI assistance. The reverse engineering of the
Kindle reading-progress storage and the implementation are produced in
collaboration with AI tooling under human review. The underlying formats and
protocols were discovered by the community research credited above, not by AI.

## License

[MIT](LICENSE) © 2026 Daniel Küster
