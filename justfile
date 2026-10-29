# KindleStoryGraph build & package tooling.

TARGET := "armv7-unknown-linux-musleabihf"
EXT := "packaging/extension"

# Cross-compile the static armhf (musl) binary via Docker.
# Produces target-armv7/<TARGET>/release/kindlestorygraph.
build:
    docker run --rm -v "$PWD":/app -w /app -v ksg-cargo-registry:/root/.cargo/registry messense/rust-musl-cross:armv7-musleabihf cargo build --release --target {{TARGET}} --target-dir /app/target-armv7

# Assemble the KUAL extension folder (requires `just build` first).
package:
    #!/usr/bin/env sh
    set -e
    mkdir -p "{{EXT}}/bin"
    cp "target-armv7/{{TARGET}}/release/kindlestorygraph" "{{EXT}}/bin/kindlestorygraph"
    cp config_example.ini "{{EXT}}/config_example.ini"
    cp manual_mappings.example.ini "{{EXT}}/manual_mappings.example.ini"
    cp allowlist.example.ini "{{EXT}}/allowlist.example.ini"
    chmod +x "{{EXT}}/bin/kindlestorygraph"
    echo "Extension assembled in {{EXT}}/ — copy it to /mnt/us/extensions/kindlestorygraph"

# Host-side static checks: format + clippy (warnings as errors) + unit tests.
lint:
    cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

# Virtual Kindle: interactive busybox shell in an emulated ARM container with
# the device file layout (requires `just build`; see docs/emulator.md).
emu-shell:
    tests/emu/emu.sh shell

# Virtual Kindle smoke suite: real armhf binary + KUAL wrapper scripts under
# QEMU, against a fixture cc.db (requires `just build`; see docs/emulator.md).
emu-test:
    tests/emu/emu.sh test
