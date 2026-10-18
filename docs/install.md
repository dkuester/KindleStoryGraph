# Installing KindleStoryGraph

*[Deutsche Übersetzung](install.de.md)*

## Prerequisites

1. A **jailbroken** Kindle (see [jailbreak.md](jailbreak.md)). Reading progress
   lives in `/var/local/cc.db`, which is only accessible with root — a jailbreak
   is a hard requirement, not optional.
2. **KUAL Next** (or KUALA) installed — see [jailbreak.md](jailbreak.md).
3. Wi-Fi connectivity on the Kindle for the sync.

## 1. Get your StoryGraph session cookies

StoryGraph has no public API, so KindleStoryGraph speaks the same web-app
endpoints your logged-in browser uses. You must copy two session cookies:

1. Log in to https://app.thestorygraph.com in a desktop browser.
2. Open DevTools → Application/Storage → Cookies → `app.thestorygraph.com`.
3. Copy the value of the cookie whose name starts with `_storygraph_session`
   → this is your `session_cookie`.
4. Copy the value of the `remember_user_token` cookie → this is your
   `remember_user_token`.

Treat these as passwords — they grant full access to your account.

## 2. Copy the extension to the Kindle

1. Connect the Kindle over USB and mount it.
2. Copy the `kindlestorygraph` extension folder to
   `/mnt/us/extensions/kindlestorygraph` (i.e. `extensions/kindlestorygraph/`
   on the USB-mounted drive). The folder contains `config.xml`, `menu.json`,
   `config_example.ini` and `bin/` (the binary + launcher scripts).
3. Safely eject the Kindle.

## 3. Configure

Either before copying (edit `config_example.ini` → rename to `config.ini`), or
after the first "Start" tap (the start script copies `config_example.ini` to
`config.ini` on first run). Fill in at least:

```ini
session_cookie=<value from step 1>
remember_user_token=<value from step 1>
```

The remaining defaults are correct for the device:

```ini
cc_db_path=/var/local/cc.db
state_path=/mnt/us/extensions/kindlestorygraph/state.json
finish_threshold=99.5
progress_step=1.0
poll_interval=900
match_threshold=0.6
min_progress=2.0
manual_mappings_path=/mnt/us/extensions/kindlestorygraph/manual_mappings.ini
```

## 4. Start the daemon

KUAL Next → KindleStoryGraph → **Start sync daemon**.

This launches the daemon in the background; it polls `cc.db` every 15 minutes
and pushes changes to StoryGraph whenever Wi-Fi is up. "Sync now" runs a single
pass immediately. "Stop sync daemon" stops it.

## 5. Verify

The daemon logs to `extensions/kindlestorygraph/kindlestorygraph.log` on the
Kindle. After a sync, check that file (and your StoryGraph shelf) to confirm
books were matched and progress was pushed.

## 6. Manual mapping for unmatched books

Books whose title can't be matched automatically show up in the log as
`no_match` and are skipped. To map one by hand:

1. Note its `cde_key` from the log (`NoMatch { cde_key: "..." }`), or find it
   with `kindlestorygraph snapshot <cc.db>`.
2. Open the book on https://app.thestorygraph.com and copy the UUID from the
   URL (`app.thestorygraph.com/books/<UUID>`), or run `search "<title>"`.
3. Add a line to `extensions/kindlestorygraph/manual_mappings.ini` (copy from
   `manual_mappings.example.ini`):

   ```
   <cde_key>=<storygraph_book_id>
   ```

The mapping takes effect on the next sync cycle and overrides any automatic
match.

## Notes & limitations

- The daemon is started manually via KUAL. It does **not** auto-start after a
  reboot yet; start it again from KUAL (upstart/Hotfix autostart is planned).
- Progress is pushed from the Kindle to StoryGraph (Kindle wins on conflict).
- First sync pushes the *current* on-device progress for every book at or above
  `min_progress` (default 2 %) — books you've only opened (Kindle jumps to
  ~1 %) are ignored. Title-matching is verified against StoryGraph, but
  `no_match` books are skipped (see the log) and can be
  [mapped manually](#6-manual-mapping-for-unmatched-books).
