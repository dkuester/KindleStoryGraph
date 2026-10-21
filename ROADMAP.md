# KindleStoryGraph — Roadmap

Grouped by effort vs. value: **Quick wins** (low effort, ship fast) first, then
**High priority** (highest value, worth the effort), then **Low priority**
(nice-to-have, safe to defer). Items tagged *(optional)* are safe to skip.

## Recently completed

- Phases 1–4: `cc.db` reader, StoryGraph client, diff-based sync engine, KUAL
  Next packaging, cross-compile tooling, install/jailbreak docs.
- Opt-in allowlist (`sync_scope=allowlist`, default) with KUAL menu selection.
- Minimum-progress threshold (`min_progress`, default 2 %).
- Manual title→book mapping (`manual_mappings.ini`).
- Manual-only mode ("Sync now") alongside the 15-minute daemon.
- First release v0.1.0 with a static armhf artifact.
- KUAL Next menu fixes: `exitmenu: false` on the per-book actions (tapping a
  book no longer closes the launcher), selection/link state shown in the entry
  label instead of KUAL's press-only `checked`, and one binary call per action
  so `menu.json` is rewritten inside the launcher's ~250 ms refresh window.
- `state.json` is written atomically (temp file → fsync → rename), and a
  corrupt file is archived as `state.json.corrupt-<epoch>` and replaced by an
  empty state instead of failing every sync cycle forever.
- Sync passes are serialised by a `flock` on `state.json.lock`, so "Sync now"
  next to the daemon can no longer clobber the other pass's state (lost
  updates); the lock fails after ~1.25 s with a clear message instead of
  blocking the launcher process.
- O0 menu polish: a `Status` group (last-sync age + synced count), count-carrying
  group headers (`Sync selection (N/M)`, `Link books (K)`), and percent in each
  book label.
- O1 host setup: `import-cookies <cookies.txt>` (Netscape export → config.ini,
  preserving other keys) and `doctor [--cc-db] [--network]`. Full plan in
  `docs/ui-options.md`.
- O6 FBInk settings UI (on-device, no browser/X): the `ui` subcommand draws the
  status block and book list straight to the e-ink framebuffer via a
  statically-linked FBInk, with tap-to-toggle sync selection, a sync-now action
  and an exit button. Touch quirks (swap/mirror axes) and the vertical viewport
  origin are honored. `build.rs` compiles vendored FBInk only for the
  armv7-musl target, so host builds stay fast.
- FBInk UI follow-ups: a read-only "Einstellungen" screen (min_progress,
  finish_threshold, language, sync_scope, and the key paths) behind a second
  button row; scrolling for long book lists (list offset + ^/v tap zones + a
  page indicator, replacing the "+N more" line); and the "Verknüpfen" flow
  (pick a book → non-blocking StoryGraph search on a background thread →
  candidate list → tap writes manual_mappings.ini and regenerates the KUAL
  menu). The search shows a "Suchen…" status with cancel; timeout, no-network,
  expired-cookies and no-results are surfaced explicitly.
- Backward-progress guard: before a progress push (and before attaching a book
  in progress) the current StoryGraph value is read; the push is skipped
  (`skipped_behind`) when the book is already read there or the remote percent
  is ahead. This also covers the status: a book read on StoryGraph is never
  demoted to "currently-reading".
- Fixed: finishing a book no longer flips it to "rereading" on StoryGraph (the
  finish path pushed 100 % after setting "read", and StoryGraph's
  update-progress flow turns a progress update on a read book into a reread).
- Per-book error isolation: a failing book is reported as `failed` and the pass
  continues; the state of the books synced before it is saved instead of being
  re-pushed on every pass. `sync` exits non-zero when any book failed.
- Atomic writes everywhere: `allowlist.ini`, `manual_mappings.ini`,
  `search_cache.json`, `menu.json`, `config.ini` and `state.json` all go
  through one shared `write_atomic` (temp file → fsync → rename), so a suspend
  or power cut mid-write can no longer truncate the allowlist and silently
  stop the sync.
- Log rotation: `start.sh` and `sync-once.sh` move `kindlestorygraph.log` to
  `kindlestorygraph.log.1` once it exceeds 512 KiB (one old file kept).

## Quick wins

- **Filter non-book entries (PDOC/dummy)**
  Exclude `p_cdeType != "EBOK"` entries (e.g. the JAILBROKEN placeholder) from
  the KUAL selection menu and the `books` output. Effort: small.

- ~~**Log rotation**~~ — done (see Recently completed).

- ~~**`doctor` command**~~ — done (see Recently completed).

## High priority

- **Remove from StoryGraph (privacy cleanup)**
  The allowlist only prevents *future* syncs — a book already synced stays on
  the shelf. Add a CLI action (and a KUAL menu entry) to reset a book to
  "to-read" on StoryGraph, completing the "keep private titles off your shelf"
  story. Thin first cut: reset status; deeper follow-up: also clear stored
  progress if a clear-progress endpoint is reachable (blocker: unverified).
  Effort: medium. Mapping: `update_status(book_id, to-read)` already exists.

- **Author-aware matching**
  Matching currently scores title only; the `j_credits` author is read but
  unused. Fold author similarity into the match score to disambiguate
  same-title books. Effort: medium (well testable with the existing mock).

- **Suspend/resume catch-up sync**
  After a suspend/resume cycle, run one immediate sync instead of waiting for
  the next poll. Effort: medium (depends on the Hotfix upstart bridge or an
  equivalent resume hook).

- **Autostart after reboot**
  Start the daemon automatically on boot via the Hotfix upstart bridge, so
  daemon users don't have to start it from KUAL after every reboot. Effort:
  medium. Less relevant for manual-only users.

## Low priority (optional)

- ~~**Cookie import helper**~~ — done as `import-cookies` (see Recently completed).

- **More statuses ("did not finish"/"paused")** *(optional)*
  Map Kindle abandonment to StoryGraph `did-not-finish`/`paused`. Feasible via
  `update_status` (statuses already supported by the client), but the Kindle
  doesn't signal DNF cleanly, so the mapping is heuristic — deferred.
