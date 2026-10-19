#!/bin/sh
# KindleStoryGraph — search StoryGraph for unlinked books, cache the candidates
# for the KUAL "Link books" picker, and regenerate menu.json.
#
# This one is slow (one HTTP search per unlinked book), so its menu entry keeps
# KUAL's default "exitmenu": true: the launcher steps aside while it runs and
# the regenerated menu is picked up the next time KUAL starts.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

exec "$EXT/bin/kindlestorygraph" find-matches
