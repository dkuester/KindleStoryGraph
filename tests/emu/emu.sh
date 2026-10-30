#!/usr/bin/env bash
# Virtual Kindle (stage 1): run the real static armv7-musl binary and the real
# KUAL wrapper scripts in an emulated ARM Linux container (QEMU via Docker)
# whose file layout matches the device, so no script needs rewriting.
#
#   tests/emu/emu.sh shell   interactive busybox shell in the emulated Kindle
#   tests/emu/emu.sh test    scripted smoke suite (exits non-zero on failure)
#
# Every run assembles a fresh scratch tree under target/emu/run/ (the repo is
# never mounted into the device paths):
#   target/emu/run/mnt/us   -> /mnt/us     (extension as `just package` lays it out)
#   target/emu/run/var/local -> /var/local (cc.db built from tests/emu/cc_fixture.sql)
#
# The smoke suite also runs one sync and one headless UI session (stage 3,
# `ui --script`) against the fake StoryGraph (stage 2,
# tests/e2e/fake_storygraph.py): the fake runs in a native python container
# with --network none, and the emulated Kindle joins that container's network
# namespace, so both share a loopback interface and nothing else — the
# binary reaches the fake at http://127.0.0.1:8080, and no container can reach
# any other host.
#
# Environment:
#   KSG_EMU_CC_DB=/path/to/cc.db  use a private cc.db dump instead of the fixture
#                                 (the fixture-specific checks are skipped)
#   KSG_EMU_NETWORK=1             give the shell network access (default: none)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
IMAGE="alpine:3.20.3"
FAKE_IMAGE="python:3.12.7-alpine3.20"
FAKE_URL="http://127.0.0.1:8080"
PLATFORM="linux/arm/v7"
BIN="$ROOT/target-armv7/armv7-unknown-linux-musleabihf/release/kindlestorygraph"
RUN="$ROOT/target/emu/run"
DEV_EXT="/mnt/us/extensions/kindlestorygraph"
EXT="$RUN$DEV_EXT"
EMU="$ROOT/tests/emu"

die() {
    echo "emu: $*" >&2
    exit 2
}

# Assemble the scratch tree: tracked extension files (with their git modes),
# the cross-built binary and example configs (as `just package` does), the
# test config.ini, and the cc.db.
prepare() {
    [ -x "$BIN" ] || die "missing $BIN — run 'just build' first"
    command -v docker >/dev/null || die "docker not found"
    command -v python3 >/dev/null || die "python3 not found"

    # On a Linux host the previous run's files are root-owned (written inside
    # the container); remove them from a container then.
    rm -rf "$RUN" 2>/dev/null ||
        docker run --rm -v "$ROOT/target/emu:/emu" "$IMAGE" rm -rf /emu/run
    mkdir -p "$EXT/bin" "$RUN/var/local"
    (cd "$ROOT" && git ls-files packaging/extension) | while read -r f; do
        dest="$EXT/${f#packaging/extension/}"
        mkdir -p "$(dirname "$dest")"
        cp -p "$ROOT/$f" "$dest"
    done
    cp "$BIN" "$EXT/bin/kindlestorygraph"
    chmod +x "$EXT/bin/kindlestorygraph"
    cp "$ROOT/config_example.ini" "$ROOT/manual_mappings.example.ini" \
        "$ROOT/allowlist.example.ini" "$EXT/"
    cp "$EMU/emu_config.ini" "$EXT/config.ini"

    if [ -n "${KSG_EMU_CC_DB:-}" ]; then
        [ -f "$KSG_EMU_CC_DB" ] || die "KSG_EMU_CC_DB=$KSG_EMU_CC_DB not found"
        cp "$KSG_EMU_CC_DB" "$RUN/var/local/cc.db"
    else
        python3 -I -c 'import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.executescript(open(sys.argv[2], encoding="utf-8").read())
c.commit()' "$RUN/var/local/cc.db" "$EMU/cc_fixture.sql"
    fi
}

DOCKER_ARGS=(--platform "$PLATFORM"
    -v "$RUN/mnt/us:/mnt/us" -v "$RUN/var/local:/var/local"
    -w "$DEV_EXT" -e 'PS1=kindle:$PWD# ')

cmd_shell() {
    prepare
    local net=(--network none)
    [ "${KSG_EMU_NETWORK:-}" = "1" ] && net=()
    echo "Virtual Kindle: $DEV_EXT (scratch: target/emu/run). Try: sh -c \"bin/toggle-book.sh B00DUNE001\""
    exec docker run --rm -it "${net[@]}" "${DOCKER_ARGS[@]}" "$IMAGE" sh
}

# ── smoke suite ──────────────────────────────────────────────────────────────

PASS=0
FAIL=0
CID=""
FAKE_CID=""

ok() {
    PASS=$((PASS + 1))
    echo "  ok    $*"
}
bad() {
    FAIL=$((FAIL + 1))
    echo "  FAIL  $*"
}
check() { # check <description> <command...>
    local desc="$1"
    shift
    if "$@"; then ok "$desc"; else bad "$desc"; fi
}

# Run a shell snippet inside the emulated Kindle (cwd = extension dir, like
# KUAL). Prints combined output; returns the snippet's exit code.
kx() {
    docker exec -w "$DEV_EXT" "$CID" sh -c "$1" 2>&1
}

menu_valid() { python3 -I -m json.tool "$EXT/menu.json" >/dev/null 2>&1; }
menu_has() { grep -qF "\"$1\"" "$EXT/menu.json"; }
menu_lacks() { ! grep -qF "$1" "$EXT/menu.json"; }
contains() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac; }
lacks() { ! contains "$1" "$2"; }

# First book key offered in the "Sync selection" submenu.
first_key() {
    python3 -I -c 'import json, sys
m = json.load(open(sys.argv[1], encoding="utf-8"))
for g in m["items"]:
    if g["name"].startswith("Sync selection"):
        print(next(i["params"] for i in g["items"] if "params" in i))' "$EXT/menu.json"
}

# Milliseconds a KUAL-style action takes inside the container, measured with
# /proc/uptime (busybox date has no %N): 10 ms resolution.
time_action() {
    kx "read a _ < /proc/uptime; sh -c \"$1\" >/dev/null 2>&1; read b _ < /proc/uptime; echo \$a \$b" |
        awk '{ printf "%.0f", ($2 - $1) * 1000 }'
}

# Every FBInk screenshot (PGM, 8 bpp) is 1236x1648 and has both black and
# white pixels, i.e. FBInk actually drew text into the fake framebuffer; the
# last one (exit) is blank white.
pgms_drawn() {
    python3 -I -c 'import pathlib, sys
pgms = sorted(pathlib.Path(sys.argv[1]).glob("*.pgm"))
assert pgms, "no .pgm frames"
for p in pgms:
    data = p.read_bytes()
    head, px = data[:17], data[17:]
    assert head == b"P5\n1236 1648\n255\n", (p, head)
    assert len(px) == 1236 * 1648, p
    if p == pgms[-1]:
        assert set(px) == {255}, (p, "exit should leave a blank white screen")
    else:
        assert 0 in px and 255 in px, (p, "no text drawn")' "$1"
}

# The fake StoryGraph's /__state, read inside its own container.
fake_state() {
    docker exec "$FAKE_CID" python -I -c 'import sys, urllib.request
sys.stdout.write(urllib.request.urlopen(sys.argv[1] + "/__state").read().decode())' "$FAKE_URL"
}
fake_ready() {
    local i
    for i in 1 2 3 4 5 6 7 8 9 10; do
        fake_state >/dev/null 2>&1 && return 0
        sleep 0.5
    done
    return 1
}

cmd_test() {
    prepare
    local fixture=1
    [ -n "${KSG_EMU_CC_DB:-}" ] && fixture=0

    local t0=$SECONDS out rc
    # The fake StoryGraph owns the (network-less) namespace; see the header.
    FAKE_CID=$(docker run -d --rm --network none -v "$ROOT/tests/e2e:/e2e:ro" "$FAKE_IMAGE" \
        python -I /e2e/fake_storygraph.py --host 127.0.0.1 --port 8080)
    trap 'docker rm -f "$CID" "$FAKE_CID" >/dev/null 2>&1 || true' EXIT
    CID=$(docker run -d --rm --network "container:$FAKE_CID" "${DOCKER_ARGS[@]}" "$IMAGE" sleep 600)
    echo "Virtual Kindle up ($IMAGE, $PLATFORM, $(kx 'uname -m'), network: loopback shared with the fake StoryGraph only)"
    [ $fixture = 1 ] || echo "(private cc.db via KSG_EMU_CC_DB: fixture-specific checks skipped)"

    echo "packaging"
    out=$(kx 'for f in bin/*; do [ -x "$f" ] || echo "not executable: $f"; done
              for f in bin/*.sh; do [ "$(head -n1 "$f")" = "#!/bin/sh" ] || echo "bad shebang: $f"; done
              for f in config.xml menu.json config.ini config_example.ini; do [ -r "$f" ] || echo "missing: $f"; done')
    check "bin/* executable, #!/bin/sh shebangs, config.xml/menu.json present${out:+ ($out)}" [ -z "$out" ]
    check "config.xml names menu.json" grep -q '<menu type="json" dynamic="true">menu.json</menu>' "$EXT/config.xml"
    check "packaged menu.json skeleton is valid JSON" menu_valid

    echo "binary"
    out=$(kx 'bin/kindlestorygraph --help') && rc=0 || rc=$?
    check "kindlestorygraph --help runs (static armv7-musl under qemu)" [ $rc = 0 ]
    check "--help lists the subcommands" contains "$out" "toggle"

    out=$(kx "KINDLE_STORYGRAPH_CONFIG=$DEV_EXT/config.ini bin/kindlestorygraph doctor") && rc=0 || rc=$?
    check "doctor exits 0" [ $rc = 0 ]
    check "doctor finds config.ini" contains "$out" "config.ini       found"
    check "doctor reads cc.db" contains "$out" "cc.db            OK"
    [ $fixture = 1 ] && check "doctor counts 7 books >= min_progress" contains "$out" "OK — 7 book(s)"

    echo "menu (bin/refresh-menu.sh)"
    out=$(kx 'sh -c "bin/refresh-menu.sh"') && rc=0 || rc=$?
    check "refresh-menu.sh exits 0" [ $rc = 0 ]
    check "menu.json is valid JSON" menu_valid
    check "status line says cookies are missing" menu_has "Cookies not set — run setup on a computer"
    check "no 'Remove from StoryGraph' without synced books" menu_lacks "Remove from StoryGraph"
    if [ $fixture = 1 ]; then
        check "header 'Sync selection (0/7)'" menu_has "Sync selection (0/7)"
        check "in-progress book listed" menu_has "Dune  42%"
        check "finished book listed (99.6 rounds to 100)" menu_has "The Martian  100%"
        check "same title, two authors: both listed" menu_has "The Outsider  55%"
        check "non-ASCII title survives" menu_has "Der Schwarm  13%"
        check "PDOC with progress listed (reader does not filter by type)" menu_has "Meeting notes 2026  15%"
        check "book below min_progress hidden" menu_lacks "Barely Opened"
        check "jailbreak dummy hidden" menu_lacks "Jailbroken"
        check "collection row hidden" menu_lacks "Sci-Fi"
    fi

    echo "toggle (bin/toggle-book.sh)"
    local key ms timings=""
    key=$(first_key)
    [ $fixture = 1 ] && check "first book is the most recent one (Dune)" [ "$key" = "B00DUNE001" ]
    ms=$(time_action "bin/toggle-book.sh $key")
    timings="$ms"
    check "toggle on: allowlist.ini contains $key" grep -qx "$key" "$EXT/allowlist.ini"
    check "toggle on: menu.json still valid" menu_valid
    check "toggle on: book marked [sync]" grep -qF "%  [sync]" "$EXT/menu.json"
    [ $fixture = 1 ] && check "toggle on: header 'Sync selection (1/7)'" menu_has "Sync selection (1/7)"
    ms=$(time_action "bin/toggle-book.sh $key")
    timings="$timings $ms"
    check "toggle off: key removed from allowlist.ini" bash -c "! grep -qx '$key' '$EXT/allowlist.ini'"
    [ $fixture = 1 ] && check "toggle off: header back to (0/7)" menu_has "Sync selection (0/7)"
    ms=$(time_action "bin/toggle-book.sh $key")
    timings="$timings $ms"
    check "toggle on again" grep -qx "$key" "$EXT/allowlist.ini"

    echo "link (bin/link-book.sh)"
    out=$(kx "sh -c \"bin/link-book.sh $key 0f1e2d3c-emu-test\"") && rc=0 || rc=$?
    check "link-book.sh exits 0" [ $rc = 0 ]
    check "manual_mappings.ini has $key=0f1e2d3c-emu-test" grep -qx "$key=0f1e2d3c-emu-test" "$EXT/manual_mappings.ini"
    check "menu marks the book [linked]" grep -qF "[sync]  [linked]" "$EXT/menu.json"

    echo "remove entry point"
    printf '{"books":{"%s":{"storygraph_id":"0f1e2d3c-emu-test","status":"currently-reading","last_percent":42.3,"last_synced_at":%s}}}\n' \
        "$key" "$(date +%s)" >"$EXT/state.json"
    kx 'sh -c "bin/refresh-menu.sh"' >/dev/null
    check "synced book -> 'Remove from StoryGraph... (1)'" menu_has "Remove from StoryGraph... (1)"
    check "remove entry opens the UI with --remove" menu_has "--remove"
    check "missing cookies still win over the last-sync status" menu_has "Cookies not set — run setup on a computer"

    echo "menu safety"
    cp "$EXT/menu.json" "$RUN/menu.before"
    mv "$RUN/var/local/cc.db" "$RUN/var/local/cc.db.away"
    out=$(kx 'sh -c "bin/refresh-menu.sh"') && rc=0 || rc=$?
    mv "$RUN/var/local/cc.db.away" "$RUN/var/local/cc.db"
    check "missing cc.db: refresh-menu.sh fails" [ $rc != 0 ]
    check "missing cc.db: menu.json left untouched" cmp -s "$RUN/menu.before" "$EXT/menu.json"
    check "missing cc.db: no 'unexpected error'" lacks "$out" "unexpected error"

    echo "log rotation"
    kx 'head -c 1000 /dev/zero > /tmp/r.log && bin/rotate-log.sh /tmp/r.log 500 && [ -f /tmp/r.log.1 ] && [ ! -f /tmp/r.log ]' >/dev/null &&
        rc=0 || rc=$?
    check "rotate-log.sh moves an oversized log to .1" [ $rc = 0 ]
    kx 'head -c 100 /dev/zero > /tmp/s.log && bin/rotate-log.sh /tmp/s.log 500 && [ -f /tmp/s.log ] && [ ! -f /tmp/s.log.1 ]' >/dev/null &&
        rc=0 || rc=$?
    check "rotate-log.sh keeps a small log" [ $rc = 0 ]
    kx 'head -c 600000 /dev/zero > kindlestorygraph.log' >/dev/null
    # The snippets below must not contain the literal "kindlestorygraph daemon":
    # start.sh/stop.sh pkill that pattern and would kill the exec'd shell
    # itself. "[d]aemon" matches the daemon but not its own command line.
    out=$(kx 'sh -c "bin/start.sh"; sleep 2; pgrep -f "kindlestorygraph [d]aemon"') && rc=0 || rc=$?
    check "start.sh starts the daemon" [ $rc = 0 ]
    check "start.sh rotated the 600 kB log to .1" [ "$(wc -c <"$EXT/kindlestorygraph.log.1" | tr -d ' ')" = 600000 ]
    check "fresh log is small" [ "$(wc -c <"$EXT/kindlestorygraph.log" | tr -d ' ')" -lt 600000 ]
    out=$(kx 'sh -c "bin/stop.sh"; sleep 1; pgrep -f "kindlestorygraph [d]aemon" || echo stopped')
    check "stop.sh stops the daemon" [ "$out" = "stopped" ]
    check "daemon log has no 'unexpected error'" bash -c "! grep -q 'unexpected error' '$EXT/kindlestorygraph.log'"

    echo "sync without cookies"
    rm -f "$EXT/state.json"
    out=$(kx "KINDLE_STORYGRAPH_CONFIG=$DEV_EXT/config.ini bin/kindlestorygraph sync --dry-run") && rc=0 || rc=$?
    check "sync --dry-run fails (exit $rc)" [ $rc != 0 ]
    check "sync --dry-run names the missing session_cookie" contains "$out" "session_cookie"
    check "sync --dry-run: no 'unexpected error'" lacks "$out" "unexpected error"
    check "sync --dry-run wrote no state.json" [ ! -e "$EXT/state.json" ]
    kx 'sh -c "bin/sync-once.sh"' >/dev/null || true
    check "sync-once.sh logs the clean message" grep -q "session_cookie" "$EXT/kindlestorygraph.log"

    echo "sync against the fake StoryGraph (stage 2)"
    if [ $fixture = 1 ]; then
        fake_ready || bad "fake StoryGraph did not start"
        sed -i.bak -e 's/^session_cookie=.*/session_cookie=fake-session-cookie/' \
            -e 's/^remember_user_token=.*/remember_user_token=fake-remember-token/' "$EXT/config.ini"
        # Fresh start (the link step above mapped Dune to a dummy id). Written
        # inside the container: on a Linux host these files are root-owned.
        kx "printf 'B00DUNE001\\nB07D7CWN1L\\n' > allowlist.ini; rm -f state.json manual_mappings.ini kindlestorygraph.log" >/dev/null
        out=$(kx "KSG_STORYGRAPH_BASE_URL=$FAKE_URL sh -c 'bin/sync-once.sh'") && rc=0 || rc=$?
        check "sync-once.sh exits 0 (exit $rc)" [ $rc = 0 ]
        out=$(cat "$EXT/kindlestorygraph.log")
        check "log: Dune attached to dune-0001" contains "$out" '"storygraph_id": "dune-0001"'
        check "log: The Outsider (King) attached to the King edition" contains "$out" '"storygraph_id": "outsider-king-0001"'
        check "log: no failures" lacks "$out" '"failed"'
        out=$(fake_state)
        check "server: Dune currently-reading at 42 %" contains "$out" '"dune-0001": {"status": "currently-reading", "percent": 42}'
        check "server: King edition at 30 %" contains "$out" '"outsider-king-0001": {"status": "currently-reading", "percent": 30}'
        check "state.json links Dune" grep -q '"dune-0001"' "$EXT/state.json"
        out=$(kx "KSG_STORYGRAPH_BASE_URL=http://example.com:8080 KINDLE_STORYGRAPH_CONFIG=$DEV_EXT/config.ini bin/kindlestorygraph whoami") && rc=0 || rc=$?
        check "non-loopback base URL refused (exit $rc)" contains "$out" "refused"

        echo "headless UI against the fake StoryGraph (stage 3)"
        kx "printf '%s\\n' 'tap [ Link ]' 'tap [ Remove ]' 'tap Dune' 'tap [ Remove ]' wait 'tap [ Exit ]' > /tmp/ui.script" >/dev/null
        # --fbink: the real FBInk renders into a fake framebuffer; the frames
        # (NNN.txt + NNN.pgm screenshots) land in target/emu/run/var/local/ui-frames.
        out=$(kx "KSG_STORYGRAPH_BASE_URL=$FAKE_URL KINDLE_STORYGRAPH_CONFIG=$DEV_EXT/config.ini bin/kindlestorygraph ui --script /tmp/ui.script --frames /var/local/ui-frames --fbink 2>&1 | grep -v 'Not running on a Kindle'") && rc=0 || rc=$?
        check "ui --script --fbink exits 0 under qemu${out:+: $out}" [ -z "$out" ]
        check "FBInk screenshots drawn (text in every .pgm, blank white on exit)" pgms_drawn "$RUN/var/local/ui-frames"
        out=$(cat "$RUN"/var/local/ui-frames/*.txt)
        check "ui frames: Remove list offers the synced book" contains "$out" "Remove from StoryGraph?"
        check "ui frames: 'Removed: Dune'" contains "$out" "Removed: Dune"
        check "server: Dune back to to-read" contains "$(fake_state)" '"dune-0001": {"status": "to-read"'
        check "allowlist no longer has Dune" bash -c "! grep -qx B00DUNE001 '$EXT/allowlist.ini'"
        mv "$EXT/config.ini.bak" "$EXT/config.ini"
    fi

    echo "ui without a framebuffer"
    out=$(kx 'sh -c "bin/ui.sh"') && rc=0 || rc=$?
    check "ui exits without a crash (exit $rc, no signal)" [ $rc -lt 128 ]
    check "ui: no panic" lacks "$out" "panicked"
    check "ui: no 'unexpected error'" lacks "$out" "unexpected error"
    check "ui: mentions the framebuffer" contains "$out" "framebuffer"

    echo
    echo "toggle-book.sh wall time (KUAL-style, under qemu): ${timings// / ms, } ms"
    echo "  Informational only: KUAL Next re-reads menu.json ~250 ms after starting the"
    echo "  action, but qemu timing on the host CPU does not predict the Kindle's Cortex-A7"
    echo "  (it can be faster or slower) — confirm the real window on the device."
    echo
    echo "$PASS passed, $FAIL failed ($((SECONDS - t0)) s)"
    [ $FAIL = 0 ]
}

case "${1:-}" in
shell) cmd_shell ;;
test) cmd_test ;;
*)
    echo "usage: $0 shell|test" >&2
    exit 2
    ;;
esac
