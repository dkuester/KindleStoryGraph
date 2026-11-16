/*
 * Headless FBInk shim (KindleStoryGraph, not part of upstream FBInk); see
 * shim.h. Compiled WITHOUT shim.h, so open()/ioctl() here are the libc ones.
 *
 * Inert by default: until ksg_shim_set_framebuffer() is called every call is
 * passed through unchanged, so the device build behaves exactly like plain
 * FBInk.
 */
#include <fcntl.h>
#include <linux/fb.h>
#include <stdarg.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

/* Kindle Paperwhite 11th gen (PW5): 1236x1648, 8 bpp grayscale. */
#define KSG_FB_WIDTH  1236U
#define KSG_FB_HEIGHT 1648U

static const char* fb_path = NULL;
static dev_t       fb_dev;
static ino_t       fb_ino;

/* Called from Rust (src/ui.rs) before fbink_open(). Creates the backing file
 * and switches the shim on. Returns 0 on success, -1 on error. */
int
    ksg_shim_set_framebuffer(const char* path)
{
	int fd = open(path, O_RDWR | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
	if (fd < 0) {
		return -1;
	}
	struct stat st;
	if (ftruncate(fd, (off_t) (KSG_FB_WIDTH * KSG_FB_HEIGHT)) != 0 || fstat(fd, &st) != 0) {
		close(fd);
		return -1;
	}
	close(fd);
	fb_dev  = st.st_dev;
	fb_ino  = st.st_ino;
	fb_path = path;
	return 0;
}

static int
    is_fake_fb(int fd)
{
	struct stat st;
	return fb_path && fstat(fd, &st) == 0 && st.st_dev == fb_dev && st.st_ino == fb_ino;
}

int
    ksg_shim_open(const char* path, int flags, ...)
{
	mode_t mode = 0;
	if (flags & O_CREAT) {
		va_list ap;
		va_start(ap, flags);
		mode = (mode_t) va_arg(ap, int);
		va_end(ap);
	}
	if (fb_path && strcmp(path, "/dev/fb0") == 0) {
		return open(fb_path, (flags & ~O_CREAT) | O_CLOEXEC);
	}
	return open(path, flags, mode);
}

/* The request type follows the libc prototype that shim.h renames. */
#ifdef __GLIBC__
typedef unsigned long ksg_ioctl_req_t;
#else
typedef int ksg_ioctl_req_t;
#endif

int
    ksg_shim_ioctl(int fd, ksg_ioctl_req_t request, ...)
{
	va_list ap;
	va_start(ap, request);
	void* arg = va_arg(ap, void*);
	va_end(ap);

	if (!is_fake_fb(fd)) {
		return ioctl(fd, request, arg);
	}
	switch ((unsigned long) (unsigned int) request) {
		case FBIOGET_VSCREENINFO: {
			struct fb_var_screeninfo* v = arg;
			memset(v, 0, sizeof(*v));
			v->xres           = KSG_FB_WIDTH;
			v->yres           = KSG_FB_HEIGHT;
			v->xres_virtual   = KSG_FB_WIDTH;
			v->yres_virtual   = KSG_FB_HEIGHT;
			v->bits_per_pixel = 8U;
			v->grayscale      = 1U;
			v->rotate         = FB_ROTATE_UR;
			return 0;
		}
		case FBIOGET_FSCREENINFO: {
			struct fb_fix_screeninfo* f = arg;
			memset(f, 0, sizeof(*f));
			strncpy(f->id, "ksg-headless", sizeof(f->id) - 1U);
			f->smem_len    = KSG_FB_WIDTH * KSG_FB_HEIGHT;
			f->line_length = KSG_FB_WIDTH;
			f->type        = FB_TYPE_PACKED_PIXELS;
			f->visual      = FB_VISUAL_STATIC_PSEUDOCOLOR;
			return 0;
		}
		default:
			/* Refresh/wait ioctls (MXCFB_*, ...): nothing to update. */
			return 0;
	}
}
