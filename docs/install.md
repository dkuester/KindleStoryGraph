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
   `config_example.ini`, `allowlist.example.ini`, `manual_mappings.example.ini`
   and `bin/` (the binary + launcher scripts).
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
sync_scope=allowlist
allowlist_path=/mnt/us/extensions/kindlestorygraph/allowlist.ini
search_cache_path=/mnt/us/extensions/kindlestorygraph/search_cache.json
menu_path=/mnt/us/extensions/kindlestorygraph/menu.json
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

## 6. Choose which books sync (allowlist)

By default (`sync_scope=allowlist`) only the books you explicitly select are
synced — nothing lands on StoryGraph unless you opt in. This is the intended
way to keep private or "spicy" titles off your shelf.

Select books in the KUAL menu:

1. Open KUAL Next → KindleStoryGraph → **Sync selection**.
2. Every book with reading progress is listed; tap one to toggle it. A
   `[sync]` suffix marks the books that will sync — KUAL's own checkmark only
   appears while a button is pressed, so the state is part of the label. The
   list is rewritten after each tap, and the menu stays open.
3. If a book you just started reading isn't listed yet, tap **Refresh book
   list** first.

The menu jumps back to its top level after each tap (KUAL Next reloads the
whole menu to pick up the new state), so a large library costs one extra tap
per book.

The selection is stored in `extensions/kindlestorygraph/allowlist.ini` (one
`cde_key` per line); you can edit that file over USB instead if you prefer.

To sync every book instead, set in `config.ini`:

```ini
sync_scope=all
```

Then all books with progress are synced and the selection is ignored.

Progress never goes backwards on StoryGraph: before each push the current
StoryGraph value is read, and the push is skipped (`skipped_behind` in the log)
when the book is already marked read there or progress made on another device
is further along. A book that failed to sync (network, expired cookies) is
logged as `failed` and retried on the next pass; the other books still sync.

## 7. Linking books to StoryGraph

Books whose title can't be matched automatically are skipped as `no_match` in
the log and stay unlinked. You can link them — or re-link a book to a
different StoryGraph book — in two ways.

### 7a. Pick from the KUAL menu (recommended)

1. In KUAL Next → KindleStoryGraph, tap **Find matches for books**. This
   searches StoryGraph (title + author) for every *unlinked* book and caches
   the top 5 results. It needs Wi-Fi and valid cookies, and it is slow (one
   search per book), so KUAL closes while it runs — reopen it when the Kindle
   is back on the home screen and **Link books** will be there.
2. Open **Link books**. Each unlinked book is listed; tap it to see the
   matching StoryGraph books (title — author), then tap the correct one. The
   menu stays open and the book disappears from **Link books**.
3. The choice is written to `manual_mappings.ini` and takes effect on the next
   sync; the book then shows a `[linked]` marker in **Sync selection**.

Books already linked (by a previous sync or a manual mapping) don't appear in
**Link books**; their entry in **Sync selection** carries the `[linked]`
marker, so you can see at a glance what's linked and what isn't.

### 7b. Edit `manual_mappings.ini` by hand

To link a book manually, or override a wrong automatic match:

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

## 8. Removing a book from StoryGraph

The allowlist only stops *future* syncs; a book that was already synced stays
on your StoryGraph shelf. To take it back off:

1. In KUAL Next → KindleStoryGraph, open **Remove from StoryGraph**. It lists
   every book this tool has synced (the group is hidden when there are none,
   and with `sync_scope=all`).
2. Tap the book. It needs Wi-Fi and valid cookies. The menu stays open and the
   book drops out of the list.

In the on-device settings UI (**Open settings UI**) the same is **Link** →
**Remove**: tap a synced book, then confirm with **Remove**. The call runs in
the background; **Back** leaves it running and the result shows up as a note.
On a host or over SSH the same is `kindlestorygraph unsync <cde_key>`.

What happens: the book is set to **to-read** on StoryGraph, then removed from
`allowlist.ini` (so the next sync doesn't put it back) and from `state.json`.
If the StoryGraph call fails (no network, expired cookies) nothing changes
locally — just try again. A manual mapping in `manual_mappings.ini` is kept on
purpose, so if you select the book again later it links to the same
StoryGraph edition.

Limitations:

- With `sync_scope=all` this is refused, because the next pass would sync the
  book again. Switch to `sync_scope=allowlist` first.
- Only the status is reset. Whether StoryGraph also drops the stored reading
  progress when a book goes back to "to-read" is not verified yet; check the
  book on StoryGraph if that matters to you.

## Notes & limitations

- The daemon is started manually via KUAL. It does **not** auto-start after a
  reboot yet; start it again from KUAL (upstart/Hotfix autostart is planned).
- Progress is pushed from the Kindle to StoryGraph (Kindle wins on conflict).
- First sync pushes the *current* on-device progress for every allowed book at
  or above `min_progress` (default 2 %) — books you've only opened (Kindle
  jumps to ~1 %) are ignored. Title-matching is verified against StoryGraph,
  but `no_match` books are skipped (see the log) and can be
  [linked](#7-linking-books-to-storygraph).
