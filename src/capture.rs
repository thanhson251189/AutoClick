//! Screen grab. Real implementation on Windows via user32/gdi32; None elsewhere.

use crate::vision::RgbImage;

pub fn grab_rect(x: i32, y: i32, w: i32, h: i32) -> Option<RgbImage> {
    #[cfg(windows)]
    {
        grab_rect_win(x, y, w, h)
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y, w, h);
        None
    }
}

/// Full virtual screen (all monitors) plus its origin: a hit in the image is
/// at `(x + hit.0, y + hit.1)` in absolute screen coordinates.
pub fn grab_screen() -> Option<(RgbImage, i32, i32)> {
    #[cfg(windows)]
    {
        const SM_XVIRTUALSCREEN: i32 = 76;
        const SM_YVIRTUALSCREEN: i32 = 77;
        const SM_CXVIRTUALSCREEN: i32 = 78;
        const SM_CYVIRTUALSCREEN: i32 = 79;
        let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if w <= 0 || h <= 0 {
            return None;
        }
        Some((grab_rect_win(x, y, w, h)?, x, y))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn GetDC(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn ReleaseDC(hwnd: *mut core::ffi::c_void, hdc: *mut core::ffi::c_void) -> i32;
    fn GetSystemMetrics(n: i32) -> i32;
}

#[cfg(windows)]
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn CreateCompatibleBitmap(
        hdc: *mut core::ffi::c_void,
        w: i32,
        h: i32,
    ) -> *mut core::ffi::c_void;
    fn SelectObject(
        hdc: *mut core::ffi::c_void,
        obj: *mut core::ffi::c_void,
    ) -> *mut core::ffi::c_void;
    fn BitBlt(
        dst: *mut core::ffi::c_void,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        src: *mut core::ffi::c_void,
        sx: i32,
        sy: i32,
        rop: u32,
    ) -> i32;
    fn GetDIBits(
        hdc: *mut core::ffi::c_void,
        hbmp: *mut core::ffi::c_void,
        start: u32,
        lines: u32,
        bits: *mut u8,
        info: *mut BitmapInfoHeader,
        usage: u32,
    ) -> i32;
    fn DeleteObject(obj: *mut core::ffi::c_void) -> i32;
    fn DeleteDC(hdc: *mut core::ffi::c_void) -> i32;
}

#[cfg(windows)]
#[repr(C)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels: i32,
    bi_y_pels: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[cfg(windows)]
fn grab_rect_win(x: i32, y: i32, w: i32, h: i32) -> Option<RgbImage> {
    if w < 2 || h < 2 {
        return None;
    }
    // Sizes in i64 with a cap: a script-supplied region must not overflow the
    // math (debug panic / wrapped allocation) or OOM the process.
    let stride = (w as i64 * 3 + 3) / 4 * 4;
    let buf_len = stride * h as i64;
    let rgb_len = w as i64 * h as i64 * 3;
    if buf_len <= 0 || rgb_len <= 0 || buf_len > (512 << 20) {
        return None;
    }
    unsafe {
        let hdc = GetDC(core::ptr::null_mut());
        if hdc.is_null() {
            return None;
        }
        let mem = CreateCompatibleDC(hdc);
        let bmp = if mem.is_null() {
            std::ptr::null_mut()
        } else {
            CreateCompatibleBitmap(hdc, w, h)
        };
        let mut info = BitmapInfoHeader {
            bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
            bi_width: w,
            bi_height: -h,
            bi_planes: 1,
            bi_bit_count: 24,
            bi_compression: 0,
            bi_size_image: 0,
            bi_x_pels: 0,
            bi_y_pels: 0,
            bi_clr_used: 0,
            bi_clr_important: 0,
        };
        let mut buf = vec![0u8; buf_len as usize];
        let mut ok = !mem.is_null() && !bmp.is_null();
        if ok {
            let old = SelectObject(mem, bmp);
            const SRCCOPY: u32 = 0x00CC_0020;
            const CAPTUREBLT: u32 = 0x4000_0000;
            let blit = BitBlt(mem, 0, 0, w, h, hdc, x, y, SRCCOPY | CAPTUREBLT) != 0;
            // MSDN: GetDIBits requires the bitmap to be deselected first, and
            // a failed call leaves the buffer black — surfacing it as None
            // beats silently matching against a black frame.
            SelectObject(mem, old);
            ok = blit && GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr(), &mut info, 0) != 0;
        }
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        if !mem.is_null() {
            DeleteDC(mem);
        }
        ReleaseDC(core::ptr::null_mut(), hdc);
        if !ok {
            return None;
        }
        let mut rgb = vec![0u8; rgb_len as usize];
        for row in 0..h as i64 {
            let off = (row * stride) as usize;
            for col in 0..w as i64 {
                let s = off + (col as usize) * 3;
                let d = ((row * w as i64 + col) * 3) as usize;
                rgb[d] = buf[s + 2];
                rgb[d + 1] = buf[s + 1];
                rgb[d + 2] = buf[s];
            }
        }
        Some(RgbImage { w, h, rgb })
    }
}
