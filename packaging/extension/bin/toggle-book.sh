#!/bin/sh
# KindleStoryGraph — toggle a book's allowlist membership, then refresh the
# KUAL menu so the checked state updates.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

"$EXT/bin/kindlestorygraph" toggle "$1" || exit 1
"$EXT/bin/kindlestorygraph" menu > "$EXT/menu.json.tmp" || exit 1
mv "$EXT/menu.json.tmp" "$EXT/menu.json"
