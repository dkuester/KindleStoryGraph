#!/bin/sh
# KindleStoryGraph — take a synced book back off StoryGraph: reset it to
# "to-read" there, drop it from the allowlist and from state.json.
#
# One binary call on purpose (see toggle-book.sh): the binary also rewrites
# menu.json inside KUAL Next's ~250 ms refresh window, so the book leaves the
# "Remove from StoryGraph" group in one step.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

exec "$EXT/bin/kindlestorygraph" unsync "$1"
