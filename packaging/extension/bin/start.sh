#!/bin/sh
# KindleStoryGraph — start the sync daemon in the background.
# KUAL runs this as root, so the daemon can read /var/local/cc.db.
EXT="/mnt/us/extensions/kindlestorygraph"
export KINDLE_STORYGRAPH_CONFIG="$EXT/config.ini"
LOG="$EXT/kindlestorygraph.log"

# Copy the example config on first run.
if [ ! -f "$KINDLE_STORYGRAPH_CONFIG" ] && [ -f "$EXT/config_example.ini" ]; then
    cp "$EXT/config_example.ini" "$KINDLE_STORYGRAPH_CONFIG"
fi

# Refresh the KUAL menu (book list + allowlist selection).
"$EXT/bin/refresh-menu.sh" 2>/dev/null

# Stop any already-running instance, then start a fresh one. Match the daemon
# only — a broad "bin/kindlestorygraph" pattern would also kill a toggle, a
# sync pass, or (when start.sh is invoked by its full path) this very shell.
pkill -f "kindlestorygraph daemon" 2>/dev/null

nohup "$EXT/bin/kindlestorygraph" daemon >> "$LOG" 2>&1 &
