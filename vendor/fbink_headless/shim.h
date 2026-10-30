/*
 * Headless FBInk shim (KindleStoryGraph, not part of upstream FBInk).
 *
 * Force-included into every FBInk translation unit by build.rs. It routes
 * FBInk's open() and ioctl() calls through ksg_shim_*() (shim.c), which pass
 * everything straight through UNLESS ksg_shim_set_framebuffer() was called:
 * then "/dev/fb0" is a plain file and the framebuffer ioctls are answered
 * with a fixed Paperwhite 11th gen geometry. That lets the real FBInk render
 * into memory with no e-ink panel (the virtual Kindle, `ui --fbink`).
 */
#ifndef KSG_FBINK_HEADLESS_SHIM_H
#define KSG_FBINK_HEADLESS_SHIM_H

/* Function-like, so only calls are renamed — and the libc prototypes, which
 * thereby declare ksg_shim_open/ksg_shim_ioctl with libc's own signature
 * (musl: ioctl(int, int, ...), glibc: ioctl(int, unsigned long, ...)).
 * Hence no prototypes here; shim.c matches the libc it is built against. */
#define open(...)  ksg_shim_open(__VA_ARGS__)
#define ioctl(...) ksg_shim_ioctl(__VA_ARGS__)

#endif
