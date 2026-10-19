#!/bin/sh
# KindleStoryGraph — regenerate the KUAL menu (book list + allowlist selection).
# --write takes the target from config menu_path and replaces the file
# atomically (temp file + rename), so the launcher never reads a partial file.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

exec "$EXT/bin/kindlestorygraph" menu --write
