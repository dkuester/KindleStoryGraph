#!/bin/sh
# KindleStoryGraph — search StoryGraph for unlinked books and cache the
# candidates for the KUAL "Link books" picker, then refresh the menu.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

"$EXT/bin/kindlestorygraph" find-matches || exit 1
"$EXT/bin/kindlestorygraph" menu > "$EXT/menu.json.tmp" || exit 1
mv "$EXT/menu.json.tmp" "$EXT/menu.json"
