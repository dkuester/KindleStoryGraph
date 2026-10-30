//! Build script: statically link FBInk (vendored in `vendor/fbink/`) for the
//! Kindle framebuffer UI.
//!
//! FBInk is a Linux e-ink framebuffer library, so it is only compiled when
//! targeting Linux. On the host (macOS) `cargo build`/`clippy`/`test` keep
//! working for the pure-Rust parts; the `ui` command is cfg-gated the same way.

use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();

    if !target.contains("linux") {
        println!("cargo:warning=FBInk UI disabled on non-Linux target '{target}'");
        return;
    }

    // The headless shim (vendor/fbink_headless): force-included into every
    // FBInk unit so its open()/ioctl() go through ksg_shim_*(), which pass
    // through unless the UI runs with `--fbink` in the virtual Kindle.
    let shim_h = format!(
        "{}/vendor/fbink_headless/shim.h",
        env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned())
    );

    let mut build = cc::Build::new();
    build
        .include("vendor/fbink")
        .flag("-include")
        .flag(&shim_h)
        .file("vendor/fbink/fbink.c")
        .file("vendor/fbink/cutef8/utf8.c")
        .file("vendor/fbink/cutef8/dfa.c")
        // Modern Kindle target. No OpenType/Image/Unifont: keeps the binary
        // small and avoids stb/libm/libunibreak dependencies.
        .define("FBINK_FOR_KINDLE", None)
        .define("FBINK_MINIMAL", None)
        .define("FBINK_WITH_DRAW", None)
        .define("FBINK_WITH_BITMAP", None)
        .define("FBINK_WITH_FONTS", None)
        .define("FBINK_WITH_INPUT", None)
        .flag_if_supported("-ffunction-sections")
        .flag_if_supported("-fdata-sections")
        .warnings(false);

    // The messense/rust-musl-cross toolchain names its compiler after the full
    // target triple (e.g. armv7-unknown-linux-musleabihf-gcc).
    if target.contains("musl") {
        build.compiler(format!("{target}-gcc"));
    }

    build.compile("fbink");

    // Compiled without the forced include: it calls the real open()/ioctl().
    // Linked after libfbink, which references it.
    let mut shim = cc::Build::new();
    shim.file("vendor/fbink_headless/shim.c").warnings(false);
    if target.contains("musl") {
        shim.compiler(format!("{target}-gcc"));
    }
    shim.compile("ksgfbinkshim");

    println!("cargo:rustc-link-lib=static=fbink");
    println!("cargo:rerun-if-changed=vendor/fbink/");
    println!("cargo:rerun-if-changed=vendor/fbink_headless/");
}
