#!/bin/sh
# KindleStoryGraph — stop the sync daemon. Matches the daemon only, so a
# running toggle/sync pass is left alone.
pkill -f "kindlestorygraph daemon" 2>/dev/null
