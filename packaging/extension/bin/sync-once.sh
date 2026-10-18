#!/bin/sh
# KindleStoryGraph — run a single sync pass now (foreground).
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"
LOG="$EXT/kindlestorygraph.log"

# Copy the example config on first run.
if [ ! -f "$KINDLE_STORYGRAPH_CONFIG" ] && [ -f "$EXT/config_example.ini" ]; then
    cp "$EXT/config_example.ini" "$KINDLE_STORYGRAPH_CONFIG"
fi

"$EXT/bin/kindlestorygraph" sync >> "$LOG" 2>&1
