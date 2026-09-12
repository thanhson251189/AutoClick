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

pub fn grab_screen() -> Option<RgbImage> {
    #[cfg(windows)]
    {
        let sw = unsafe { GetSystemMetrics(0) };
        let sh = unsafe { GetSystemMetrics(1) };
        grab_rect(0, 0, sw, sh)
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
    unsafe {
        let hdc = GetDC(core::ptr::null_mut());
        if hdc.is_null() {
            return None;
        }
        let mem = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, bmp);
        const SRCCOPY: u32 = 0x00CC_0020;
        BitBlt(mem, 0, 0, w, h, hdc, x, y, SRCCOPY);
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
        let stride = ((w * 3 + 3) / 4) * 4;
        let mut buf = vec![0u8; (stride * h) as usize];
        GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr(), &mut info, 0);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(core::ptr::null_mut(), hdc);
        let mut rgb = vec![0u8; (w * h * 3) as usize];
        for row in 0..h {
            let off = (row * stride) as usize;
            for col in 0..w {
                let s = off + (col as usize) * 3;
                let d = ((row * w + col) * 3) as usize;
                rgb[d] = buf[s + 2];
                rgb[d + 1] = buf[s + 1];
                rgb[d + 2] = buf[s];
            }
        }
        Some(RgbImage { w, h, rgb })
    }
}
