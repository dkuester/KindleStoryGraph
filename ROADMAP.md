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

## Quick wins

- **Backward-progress guard** *(high value)*
  Before pushing a progress update, read the current StoryGraph value and skip
  the push when the Kindle value is lower — so progress made on another device
  never regresses ("StoryGraph runs backward"). Thin first cut: compare percent
  only; deeper follow-up: also guard the status (don't demote "read" back to
  "currently-reading"). Effort: small. Mapping: `get_user_book` is already in
  the client (`userbook` command).

- **Concurrent-sync guard (state.json race)**
  "Sync now" while the daemon runs can race on `state.json`. Write via
  tmp-file + atomic rename (or a lock file). Effort: small.

- **Filter non-book entries (PDOC/dummy)**
  Exclude `p_cdeType != "EBOK"` entries (e.g. the JAILBROKEN placeholder) from
  the KUAL selection menu and the `books` output. Effort: small.

- **Log rotation**
  Rotate `kindlestorygraph.log` by size so it doesn't grow unbounded.
  Effort: small.

- **`doctor` command** *(optional)*
  One command that checks `cc.db` is readable, `config.ini` is valid, and the
  session cookies still work. Effort: small.

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

- **Cookie import helper** *(optional)*
  Extract `session_cookie` + `remember_user_token` from an exported browser
  cookie file instead of copying values out of DevTools by hand.

- **More statuses ("did not finish"/"paused")** *(optional)*
  Map Kindle abandonment to StoryGraph `did-not-finish`/`paused`. Feasible via
  `update_status` (statuses already supported by the client), but the Kindle
  doesn't signal DNF cleanly, so the mapping is heuristic — deferred.
