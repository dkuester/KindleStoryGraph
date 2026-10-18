# KindleStoryGraph

StoryGraph sync for the native Kindle reader — the Kindle counterpart to
[NickelStoryGraph](https://github.com/dkuester/NickelStorygraph) (Kobo/Tolino).

Synchronizes your reading progress and reading status (currently reading /
finished) from a jailbroken Kindle to
[The StoryGraph](https://thestorygraph.com), without switching reader apps.

> **Status:** early development — Phases 1 and 2 done. The `cc.db` reader and
> the StoryGraph web-API client (login, search, status, progress) are
> implemented and unit-tested; the on-device daemon (Phase 3) is next. See
> [docs/architecture.md](docs/architecture.md).

*[Deutsche Übersetzung](README.de.md)*

## How it works

Unlike Kobo, the Kindle reader cannot be hooked in-process. KindleStoryGraph
runs as a small daemon that reads the Kindle content catalog (`cc.db`),
extracts the current book and its percentage, and pushes it to StoryGraph over
the same HTTP layer NickelStoryGraph uses.

- Native reader only — no KOReader required.
- Read-only: never touches `cc.db` or your books.
- Sparse polling, Wi-Fi gated, suspend-aware.

## Requirements

- A jailbroken Kindle (e.g. [Véra](https://kindlemodding.org/jailbreaking/Vera/)).
- Firmware ~5.17+ (developed and tested on 5.20.1, Kindle Basic 2024).

## Project status

Phase 0. See [docs/architecture.md](docs/architecture.md) for the full plan and
implementation phases.

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
- [KindleModding](https://kindlemodding.org/) — Véra jailbreak, Hotfix, KUAL,
  MRPI and KPM, the infrastructure this project depends on.
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
