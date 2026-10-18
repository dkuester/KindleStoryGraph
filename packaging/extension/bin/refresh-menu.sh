#!/bin/sh
# KindleStoryGraph — regenerate the KUAL menu (book list + allowlist selection).
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

"$EXT/bin/kindlestorygraph" menu > "$EXT/menu.json.tmp" || exit 1
mv "$EXT/menu.json.tmp" "$EXT/menu.json"
