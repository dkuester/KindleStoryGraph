//! Minimal FFI bindings to FBInk (vendored in `vendor/fbink/`, statically linked).
//!
//! Only the subset of the API the settings UI needs is exposed. Struct field
//! offsets were verified against GCC 12 for `armv7-unknown-linux-musleabihf`
//! (see the layout dump used during development); `#[repr(C)]` reproduces the
//! C layout for these primitive types exactly.

use std::ffi::{c_char, c_int, c_long, c_short, c_ushort};

/// Magic `fbfd` value: let FBInk open/close the framebuffer per call.
pub const FBFD_AUTO: c_int = -1;

// FONT_INDEX_E (fixed-cell fonts bundled with FBInk)
pub const FONT_IBM: u8 = 0;
pub const FONT_TERMINUS: u8 = 19;
pub const FONT_TERMINUSB: u8 = 20;
pub const FONT_SPLEEN: u8 = 22;
pub const FONT_COZETTE: u8 = 30;

// ALIGN_INDEX_E
pub const ALIGN_NONE: u8 = 0;
pub const ALIGN_CENTER: u8 = 1;
pub const ALIGN_EDGE: u8 = 2;

// eInk palette (FG_COLOR_INDEX_E / BG_COLOR_INDEX_E)
pub const FG_BLACK: u8 = 0;
pub const FG_WHITE: u8 = 15;
pub const BG_WHITE: u8 = 0;
pub const BG_BLACK: u8 = 15;

// WFM_MODE_INDEX_E (waveform refresh modes)
pub const WFM_AUTO: u8 = 0;
pub const WFM_DU: u8 = 1;
pub const WFM_GC16: u8 = 2;
pub const WFM_A2: u8 = 4;
pub const WFM_GL16: u8 = 5;

// INPUT_DEVICE_TYPE_E (bitmask)
pub const INPUT_TOUCHSCREEN: u32 = 1 << 3;
pub const INPUT_TABLET: u32 = 1 << 5;
pub const INPUT_KEY: u32 = 1 << 6;
pub const INPUT_POWER_BUTTON: u32 = 1 << 16;
pub const INPUT_PAGINATION_BUTTONS: u32 = 1 << 18;
pub const INPUT_HOME_BUTTON: u32 = 1 << 19;

// INPUT_SETTINGS_TYPE_E
pub const SCAN_ONLY: u32 = 1 << 0;
pub const OPEN_BLOCKING: u32 = 1 << 1;
pub const NO_RECAP: u32 = 1 << 4;

// linux/input.h event types (subset)
pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_ABS: u16 = 3;

// Absolute axes (multitouch protocol)
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_PRESSURE: u16 = 0x3a;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;

// Keys / buttons
pub const BTN_TOUCH: u16 = 0x14a;
pub const KEY_POWER: u16 = 116;

/// Mirror of FBInk's `FBInkConfig`. Zero-initialized is a sane default, which
/// is exactly what FBInk expects ("Perfectly sane when fully zero-initialized").
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FBInkConfig {
    pub row: c_short,           // 0
    pub col: c_short,           // 2
    pub fontmult: u8,           // 4
    pub fontname: u8,           // 5
    pub is_inverted: bool,      // 6
    pub is_flashing: bool,      // 7
    pub is_cleared: bool,       // 8
    pub is_centered: bool,      // 9
    pub hoffset: c_short,       // 10
    pub voffset: c_short,       // 12
    pub is_halfway: bool,       // 14
    pub is_padded: bool,        // 15
    pub is_rpadded: bool,       // 16
    pub fg_color: u8,           // 17
    pub bg_color: u8,           // 18
    pub is_overlay: bool,       // 19
    pub is_bgless: bool,        // 20
    pub is_fgless: bool,        // 21
    pub no_viewport: bool,      // 22
    pub is_verbose: bool,       // 23
    pub is_quiet: bool,         // 24
    pub ignore_alpha: bool,     // 25
    pub halign: u8,             // 26
    pub valign: u8,             // 27
    pub scaled_width: c_short,  // 28
    pub scaled_height: c_short, // 30
    pub wfm_mode: u8,           // 32
    pub dithering_mode: u8,     // 33
    pub sw_dithering: bool,     // 34
    pub cfa_mode: u8,           // 35
    pub is_nightmode: bool,     // 36
    pub no_refresh: bool,       // 37
    pub no_merge: bool,         // 38
    pub is_animated: bool,      // 39
    pub saturation_boost: u8,   // 40
    pub to_syslog: bool,        // 41
}

impl Default for FBInkConfig {
    fn default() -> Self {
        // SAFETY: FBInkConfig is an all-integer/bool C struct; zero is valid
        // for every field and is FBInk's documented default.
        unsafe { std::mem::zeroed() }
    }
}

/// Mirror of FBInk's `FBInkRect` (x before y, unlike an mxcfb rect).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FBInkRect {
    pub left: c_ushort,
    pub top: c_ushort,
    pub width: c_ushort,
    pub height: c_ushort,
}

/// Mirror of FBInk's `FBInkState` (full 180-byte layout on armv7; verified
/// against a GCC `-S` dump with the exact `build.rs` flags). The trailing
/// fields after `max_rows` matter: `fbink_get_state` writes the *entire*
/// struct, so a truncated mirror would overflow the stack. The touch-quirk
/// fields let us map raw panel coordinates back to framebuffer coordinates.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FBInkState {
    pub user_hz: c_long,               // 0
    pub font_name: *const c_char,      // 4
    pub view_width: u32,               // 8
    pub view_height: u32,              // 12
    pub screen_width: u32,             // 16
    pub screen_height: u32,            // 20
    pub scanline_stride: u32,          // 24
    pub bpp: u32,                      // 28
    pub inverted_grayscale: bool,      // 32
    pub device_name: [u8; 32],         // 33
    pub device_codename: [u8; 32],     // 65
    pub device_platform: [u8; 32],     // 97
    pub device_id: u16,                // 130
    pub pen_fg_color: u8,              // 132
    pub pen_bg_color: u8,              // 133
    pub screen_dpi: u16,               // 134
    pub font_w: u16,                   // 136
    pub font_h: u16,                   // 138
    pub max_cols: u16,                 // 140
    pub max_rows: u16,                 // 142
    pub view_hori_origin: u8,          // 144
    pub view_vert_origin: u8,          // 145
    pub view_vert_offset: u8,          // 146
    pub fontsize_mult: u8,             // 147
    pub glyph_width: u8,               // 148
    pub glyph_height: u8,              // 149
    pub is_perfect_fit: bool,          // 150
    _pad1: [u8; 15],                   // 151..165 (SoC/rota quirk fields we don't read)
    pub touch_swap_axes: bool,         // 166
    pub touch_mirror_x: bool,          // 167
    pub touch_mirror_y: bool,          // 168
    _pad2: u8,                         // 169 (is_ntx_quirky_landscape)
    pub current_rota: u8,              // 170
    pub can_rotate: bool,              // 171
    _pad3: [u8; 3],                    // 172..174 (can_hw_invert, has_eclipse_wfm, has_color_panel)
    pub pixel_format: u8,              // 175
    pub can_wait_for_submission: bool, // 176
    _pad4: [u8; 3],                    // 177..179 (end padding to 180)
}

/// Mirror of FBInk's `FBInkInputDevice` (what `fbink_input_scan` returns).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FBInkInputDevice {
    pub device_type: u32, // 0
    pub fd: c_int,        // 4
    pub matched: bool,    // 8
    pub name: [u8; 256],  // 9
    pub path: [u8; 4096], // 265
}

/// A single Linux `input_event` as read from `/dev/input/event*` on armv7
/// (kernel ABI: 16 bytes, 32-bit timeval).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputEvent {
    pub time: [u8; 8], // opaque timeval (tv_sec + tv_usec)
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

#[link(name = "fbink", kind = "static")]
#[link(name = "ksgfbinkshim", kind = "static")]
unsafe extern "C" {
    pub fn fbink_open() -> c_int;
    pub fn fbink_close(fbfd: c_int) -> c_int;
    pub fn fbink_init(fbfd: c_int, cfg: *const FBInkConfig) -> c_int;
    pub fn fbink_print(fbfd: c_int, string: *const c_char, cfg: *const FBInkConfig) -> c_int;
    pub fn fbink_refresh(
        fbfd: c_int,
        region_top: u32,
        region_left: u32,
        region_width: u32,
        region_height: u32,
        cfg: *const FBInkConfig,
    ) -> c_int;
    pub fn fbink_cls(
        fbfd: c_int,
        cfg: *const FBInkConfig,
        rect: *const FBInkRect,
        no_rota: bool,
    ) -> c_int;
    pub fn fbink_get_state(cfg: *const FBInkConfig, state: *mut FBInkState);
    pub fn fbink_fill_rect_gray(
        fbfd: c_int,
        cfg: *const FBInkConfig,
        rect: *const FBInkRect,
        no_rota: bool,
        y: u8,
    ) -> c_int;
    /// The headless shim (vendor/fbink_headless/shim.c): back `/dev/fb0`
    /// with the file at `path` from now on. 0 on success.
    pub fn ksg_shim_set_framebuffer(path: *const c_char) -> c_int;
    pub fn fbink_input_scan(
        match_types: u32,
        exclude_types: u32,
        settings: u32,
        dev_count: *mut usize,
    ) -> *mut FBInkInputDevice;
}

/// Compile-time sanity checks of the hand-verified C layout. These run only
/// when the module is compiled for the target (the module is cfg-gated to
/// armv7 Linux), so `c_long` is 32-bit and the offsets must match the dump.
#[allow(dead_code)]
const fn assert_layout() {
    use std::mem::{align_of, offset_of, size_of};
    const fn check(cond: bool) {
        if !cond {
            panic!("FBInk layout mismatch");
        }
    }
    check(size_of::<FBInkConfig>() == 42);
    check(offset_of!(FBInkConfig, hoffset) == 10);
    check(offset_of!(FBInkConfig, voffset) == 12);
    check(offset_of!(FBInkConfig, fg_color) == 17);
    check(offset_of!(FBInkConfig, bg_color) == 18);
    check(offset_of!(FBInkConfig, halign) == 26);
    check(offset_of!(FBInkConfig, scaled_width) == 28);
    check(offset_of!(FBInkConfig, scaled_height) == 30);
    check(offset_of!(FBInkConfig, wfm_mode) == 32);
    check(offset_of!(FBInkConfig, cfa_mode) == 35);
    check(offset_of!(FBInkConfig, is_nightmode) == 36);
    check(offset_of!(FBInkConfig, no_refresh) == 37);
    check(offset_of!(FBInkConfig, saturation_boost) == 40);
    check(offset_of!(FBInkConfig, to_syslog) == 41);
    check(size_of::<FBInkRect>() == 8);
    check(offset_of!(FBInkRect, width) == 4);
    check(size_of::<FBInkState>() == 180);
    check(offset_of!(FBInkState, screen_width) == 16);
    check(offset_of!(FBInkState, screen_height) == 20);
    check(offset_of!(FBInkState, device_name) == 33);
    check(offset_of!(FBInkState, font_w) == 136);
    check(offset_of!(FBInkState, font_h) == 138);
    check(offset_of!(FBInkState, max_cols) == 140);
    check(offset_of!(FBInkState, max_rows) == 142);
    check(offset_of!(FBInkState, view_vert_origin) == 145);
    check(offset_of!(FBInkState, glyph_width) == 148);
    check(offset_of!(FBInkState, touch_swap_axes) == 166);
    check(offset_of!(FBInkState, touch_mirror_x) == 167);
    check(offset_of!(FBInkState, touch_mirror_y) == 168);
    check(size_of::<FBInkInputDevice>() == 4364);
    check(offset_of!(FBInkInputDevice, fd) == 4);
    check(offset_of!(FBInkInputDevice, matched) == 8);
    check(offset_of!(FBInkInputDevice, name) == 9);
    check(offset_of!(FBInkInputDevice, path) == 265);
    check(align_of::<FBInkConfig>() == 2);
    check(size_of::<InputEvent>() == 16);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbink_layout_matches_c() {
        assert_layout();
    }
}
