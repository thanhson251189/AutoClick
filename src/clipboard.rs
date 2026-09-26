//! Clipboard text (Windows user32/kernel32). AMK-style Set/Get Clipboard.
//! OpenClipboard fails while another app holds it, so opens retry briefly.

#[cfg(windows)]
const CF_UNICODETEXT: u32 = 13;
#[cfg(windows)]
const GMEM_MOVEABLE: u32 = 0x0002;

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(owner: *mut core::ffi::c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, handle: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GetClipboardData(format: u32) -> *mut core::ffi::c_void;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
    fn GlobalLock(mem: *mut core::ffi::c_void) -> *mut u16;
    fn GlobalUnlock(mem: *mut core::ffi::c_void) -> i32;
    fn GlobalFree(mem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}

#[cfg(windows)]
/// Opens the clipboard, retrying while other apps hold it.
fn open_clipboard() -> bool {
    for _ in 0..20 {
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// Put `text` on the clipboard as CF_UNICODETEXT.
pub fn set_text(text: &str) -> bool {
    #[cfg(windows)]
    {
        if text.is_empty() {
            return false;
        }
        if !open_clipboard() {
            return false;
        }
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * 2;
        unsafe {
            EmptyClipboard();
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if mem.is_null() {
                CloseClipboard();
                return false;
            }
            let dst = GlobalLock(mem);
            if dst.is_null() {
                GlobalFree(mem);
                CloseClipboard();
                return false;
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            GlobalUnlock(mem);
            // Ownership transfers to the clipboard; do not GlobalFree.
            let ok = !SetClipboardData(CF_UNICODETEXT, mem).is_null();
            CloseClipboard();
            ok
        }
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        false
    }
}

/// Current clipboard text, if it holds CF_UNICODETEXT.
pub fn get_text() -> Option<String> {
    #[cfg(windows)]
    {
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
            return None;
        }
        if !open_clipboard() {
            return None;
        }
        let result = unsafe {
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                CloseClipboard();
                return None;
            }
            let src = GlobalLock(handle);
            if src.is_null() {
                CloseClipboard();
                return None;
            }
            // Wide string: stop at the terminator, with a hard cap.
            let cap = 1 << 20;
            let mut len = 0usize;
            while len < cap && *src.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(src, len));
            GlobalUnlock(handle);
            text
        };
        unsafe { CloseClipboard() };
        Some(result)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn set_then_get_roundtrips() {
        let text = format!("amk-clipboard-{}", std::process::id());
        assert!(set_text(&text), "set_text failed");
        assert_eq!(get_text().as_deref(), Some(text.as_str()));
    }
}
