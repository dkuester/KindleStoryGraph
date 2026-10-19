#!/bin/sh
# KindleStoryGraph — toggle a book's allowlist membership.
#
# One binary call on purpose: the binary also rewrites menu.json, and KUAL Next
# re-reads it only ~250 ms after starting the action (and only because the menu
# entry sets "exitmenu": false). A second process in that window would lose the
# race and leave the old checkmarks on screen.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

exec "$EXT/bin/kindlestorygraph" toggle "$1"
