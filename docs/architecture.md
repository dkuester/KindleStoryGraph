# KindleStoryGraph — Architecture

Status: **Phases 1–4 implemented** (reader, StoryGraph client, sync engine +
daemon loop, KUAL packaging + cross-compile tooling + install/jailbreak docs).
Only the on-device live sync test remains. This document is the implementable
plan; it is refined as each phase lands.

## 1. Goal

Sync reading progress and reading status from the **native Kindle reader** to
**The StoryGraph**, on a jailbroken Kindle. KindleStoryGraph is the Kindle
counterpart of [NickelStoryGraph](https://github.com/dkuester/NickelStorygraph)
(Kobo/Tolino).

## 2. Constraints (established during feasibility)

1. **No in-process hook.** Kindle has no NickelHook equivalent; the native
   reader (WebKit/Java "booklet" framework) cannot be symbol-hooked the way
   `libnickel.so` can on Kobo. Data is gathered by **polling**, not signals.
2. **Progress lives in `cc.db`.** `/var/local/cc.db` (SQLite), held open by
   `KPPMainApp`. `p_percentFinished` is the authoritative percentage;
   `p_readState` is legacy and inconsistent. Read-only polling of `cc.db`.
3. **`cc.db` is not visible over USB/MTP** — reading it requires root, so the
   jailbreak (Véra) is a hard prerequisite.
4. **StoryGraph has no stable public API.** Reuse NickelStoryGraph's web-app
   client (CSRF login + internal JSON endpoints).
5. **E-ink device.** Battery and network must be respected: sparse polling,
   Wi-Fi gating, one-shot work then sleep.

## 3. Component overview

```
               +--------------------------------+
               |             cc.db              |  /var/local/cc.db (SQLite, read-only)
               +--------------------------------+
                                 |
                                 v
               +--------------------------------+
               |  1. data source                |  snapshot query: title, author,
               |     cc.db query + dedup        |  percent, lastAccess, cdeKey
               +--------------------------------+
                                 |
                                 v
               +--------------------------------+
               |  2. state machine              |  local state store, delta/throttle,
               |     + local state (JSON)       |  status transitions, finish threshold
               +--------------------------------+
                                 |
                                 v
               +--------------------------------+
               |  3. StoryGraph client          |  login (CSRF + cookies), book
               |     (port of NickelStoryGraph) |  matching, status + progress update
               +--------------------------------+
                                 |
                                 v
               +--------------------------------+
               |  4. daemon / scheduler         |  upstart on framework_ready, poll
               |                                |  loop, Wi-Fi gating, suspend/resume
               +--------------------------------+

Packaging: KUAL/MRPI extension + config.ini + logging.
```

## 4. Data source: reading `cc.db`

Read-only. Never write to `cc.db` — `KPPMainApp` owns it, the catalog can hold
duplicate rows, and writing risks corruption. The only mutable state this
project keeps is its own local state store (§5).

Snapshot query (one poll cycle):

```sql
SELECT p_cdeKey, p_cdeType, p_titles_0_nominal, j_credits,
       p_percentFinished, p_lastAccess, p_lastAccessedPosition, p_readState
FROM Entries
WHERE p_type = 'Entry:Item'
  AND p_percentFinished IS NOT NULL
  AND p_percentFinished > 0;
```

- Author: `json_extract(j_credits, '$[0].name.display')`.
- `p_lastAccess` is epoch seconds.
- `p_percentFinished` is a **float** (e.g. `99.6`); round only for display,
  never for threshold logic.
- `min_progress` (default `2.0`): a book is only considered once its percent
  reaches this threshold, so books the Kindle marks ~1 % on first open are
  ignored (applied in Rust, after the SQL above).

Deduplication: the catalog can hold more than one row per book (e.g. cloud vs
local). Key books by `p_cdeKey` where unique; when duplicate rows share a
title/ASIN, keep the row with the greatest `p_lastAccess` and drop the rest.
The exact duplicate pattern is pinned down in Phase 1 on a real device.

WAL: open with `SQLITE_OPEN_READONLY` (or `PRAGMA query_only=ON`). SQLite
allows concurrent readers while `KPPMainApp` writes via WAL. The `icu`
collation on `p_titles_0_collation` / `p_credits_0_name_collation` only
matters for writes — reads are unaffected.

Schema source: reverse-engineered and documented by
[zevisvei/kindle-reading-dashboard](https://github.com/zevisvei/kindle-reading-dashboard)
(`docs/cc-db-schema.md`, `docs/read-state-storage.md`). Verified unchanged on
firmware 5.20.1 (Kindle Basic 2024, KT6).

## 5. State machine & sync logic

Local state (persisted JSON, e.g.
`/mnt/us/extensions/kindlestorygraph/state.json`):

```json
{
  "books": {
    "<cdeKey>": {
      "storygraphId": "…",
      "status": "currently-reading",
      "lastPercent": 65.0,
      "lastSyncedAt": 1729000000
    }
  }
}
```

Status derivation from **percent** (NOT from `p_readState`):

| percent                                | StoryGraph status  |
|----------------------------------------|--------------------|
| `>= finish_threshold` (default 99.5)   | read (finished)    |
| `min_progress <= pct < finish_threshold` | currently-reading|
| `< min_progress`, `0` or `NULL`        | ignored (not synced) |

"Currently reading" = the in-progress book (`min_progress <= pct < threshold`)
with the greatest `p_lastAccess`.

Sync rules (idempotent, throttled):

- StoryGraph id resolution: a manual mapping (`manual_mappings.ini`) wins, then
  the persisted state, then an automatic title search.
- First observation of a book → create/attach the StoryGraph entry, set
  currently-reading.
- Percent delta `>= progress_step` (default `1.0` percentage point) since last
  sync → push a progress update.
- Crossing `finish_threshold` → set read + final progress.
- No change → no network call.

Why percent, not `p_readState`: `p_readState` is legacy. On 5.20.1 it carries
extra values (`3`/`4`/`6`) not seen on 5.18.1, and a manual "Mark as Read" does
not update it (it lands in `fmcache.db` + cloud, per the kindle-reading-dashboard
docs). A book can show `p_readState=2` at 34 %. Percent is the only reliable
signal.

## 6. StoryGraph client

Port NickelStoryGraph's Rust client, unchanged in behaviour:

- **login:** `GET /users/sign_in` (CSRF) → `POST user[email]`/`user[password]`,
  capture `_storygraph_session` + `remember_user_token`; password via stdin,
  never argv.
- **book matching:** search StoryGraph by title + author (primary). Kindle
  `p_cdeKey` is an ASIN (store EBOK) or a UUID (PDOC/sideloaded); ASIN is not
  reliably resolvable to a StoryGraph edition, so title+author matching is the
  primary path with a manual override in config for ambiguous matches.
- **update:** set status (currently-reading / read) and progress.

Known blocker (inherited from NickelStoryGraph): `/users/sign_in` sits behind a
Cloudflare bot challenge for non-browser clients; the working fallback is
copying the session cookie from a browser into `config.ini`.

Cross-compile target: **armhf** (Kindle Basic 2024 / KT6 uses hard-float,
FW ≥ 5.16.3). Toolchain `arm-linux-gnueabihf-` (vs Kobo's
`arm-kobo-linux-gnueabihf-`).

## 7. Daemon & scheduler

- Started on `framework_ready` (Hotfix upstart bridge) or manually from KUAL.
- Poll loop: default interval 15 min, only when `wlan0` is up and online.
- On resume from suspend, run one catch-up sync.
- One-shot semantics per wake (query → diff → sync → sleep); never a busy-loop,
  and no wake-lock held across the network call.

## 8. Packaging

- KUAL extension entry, installable via MRPI/KPM.
- `config.ini`: email (or session cookie), `finish_threshold`, `progress_step`,
  `poll_interval`, `language`.
- Log to a file under `/mnt/us/extensions/kindlestorygraph/` (and `logread`).

## 9. Implementation phases

- **Phase 0** (this doc): architecture + repo scaffolding.
- **Phase 1** ✅: `cc.db` reader — host prototype against a `cc.db` dump, then
  on-device read-only access; pin down dedup + float handling.
- **Phase 2** ✅: StoryGraph client — port NickelStoryGraph's Rust CLI to armhf;
  test login + matching + updates against StoryGraph.
- **Phase 3** ✅: daemon + state machine; host unit tests for the state machine.
- **Phase 4** ✅: packaging (KUAL/MRPI/KPM) + install guide + jailbreak
  prerequisite doc + i18n (EN/DE). Live sync test still pending on-device.

## 10. Risks & open questions

- StoryGraph login Cloudflare challenge (inherited) — needs the cookie
  fallback.
- `cc.db` read consistency under WAL — verify on-device (Phase 1).
- Duplicate `Entries` rows — exact semantics TBD in Phase 1.
- Multi-device conflict (Kobo + Kindle both syncing to StoryGraph) — define
  precedence; default last-writer-wins.
- Poll interval vs battery — tunable, default conservative.
- `p_readState` values `3`/`4`/`6` on 5.20.1 — not blocking (percent-based),
  worth understanding later.
