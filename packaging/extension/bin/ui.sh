#!/bin/sh
# KindleStoryGraph — open the on-device settings UI (FBInk, framebuffer).
# Runs as the launcher process (root) so it can read /var/local/cc.db.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"

# Copy the example config on first run.
if [ ! -f "$KINDLE_STORYGRAPH_CONFIG" ] && [ -f "$EXT/config_example.ini" ]; then
    cp "$EXT/config_example.ini" "$KINDLE_STORYGRAPH_CONFIG"
fi

exec "$EXT/bin/kindlestorygraph" ui
