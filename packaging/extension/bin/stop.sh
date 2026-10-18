#!/bin/sh
# KindleStoryGraph — stop the sync daemon.
EXT="/mnt/us/extensions/kindlestorygraph"
pkill -f "$EXT/bin/kindlestorygraph" 2>/dev/null
