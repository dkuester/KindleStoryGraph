#!/bin/sh
# KindleStoryGraph — size-based log rotation: rotate-log.sh <logfile> [max_bytes]
# Moves <logfile> to <logfile>.1 (replacing an older .1) once it exceeds
# max_bytes (default 512 KiB), so the log on /mnt/us can't grow unbounded.
# Busybox/POSIX sh only: `wc -c` instead of `stat -c`.
#
# Called by start.sh and sync-once.sh right before they append. The daemon
# keeps its `>> log` fd open for as long as it runs, so a rotation while it is
# alive only renames the file under it (it goes on writing into .1) — the
# realistic rotation point is daemon start.
LOG="$1"
MAX="${2:-524288}"

[ -n "$LOG" ] && [ -f "$LOG" ] || exit 0
SIZE=$(wc -c < "$LOG" 2>/dev/null | tr -d ' ')
[ -n "$SIZE" ] || exit 0
if [ "$SIZE" -gt "$MAX" ]; then
    mv -f "$LOG" "$LOG.1"
fi
exit 0
