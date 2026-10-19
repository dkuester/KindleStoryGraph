#!/bin/sh
# KindleStoryGraph — link a Kindle book (cde_key) to a StoryGraph book id in
# manual_mappings.ini, then refresh the KUAL menu.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

"$EXT/bin/kindlestorygraph" link "$1" "$2" || exit 1
"$EXT/bin/kindlestorygraph" menu > "$EXT/menu.json.tmp" || exit 1
mv "$EXT/menu.json.tmp" "$EXT/menu.json"
