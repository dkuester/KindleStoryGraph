#!/bin/sh
# KindleStoryGraph — link a Kindle book (cde_key) to a StoryGraph book id in
# manual_mappings.ini. The binary also rewrites menu.json (same ~250 ms window
# as toggle-book.sh), so the book disappears from "Link books" and gains the
# [linked] marker in one step.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

exec "$EXT/bin/kindlestorygraph" link "$1" "$2"
