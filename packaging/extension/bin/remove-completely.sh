#!/bin/sh
# KindleStoryGraph — remove a synced book from StoryGraph entirely
# (StoryGraph's "Remove book": reading history AND any review are deleted),
# then drop it from the allowlist and from state.json.
#
# One binary call on purpose (see toggle-book.sh): the binary also rewrites
# menu.json inside KUAL Next's ~250 ms refresh window.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

exec "$EXT/bin/kindlestorygraph" unsync --completely "$1"
