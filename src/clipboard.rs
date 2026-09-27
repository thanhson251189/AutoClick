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
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
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

/// Put `html` on the clipboard in the Windows "HTML Format" (CF_HTML).
/// Paste targets (Word, editors, browsers) read it as rich text.
pub fn set_html(html: &str) -> bool {
    #[cfg(windows)]
    {
        let payload = build_cf_html(html);
        let fmt = html_format_id();
        if fmt == 0 || !open_clipboard() {
            return false;
        }
        let bytes = payload.len() + 1; // NUL terminator
        unsafe {
            EmptyClipboard();
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if mem.is_null() {
                CloseClipboard();
                return false;
            }
            let dst = GlobalLock(mem) as *mut u8;
            if dst.is_null() {
                GlobalFree(mem);
                CloseClipboard();
                return false;
            }
            std::ptr::copy_nonoverlapping(payload.as_ptr(), dst, payload.len());
            *dst.add(payload.len()) = 0;
            GlobalUnlock(mem);
            // Ownership transfers to the clipboard; do not GlobalFree.
            let ok = !SetClipboardData(fmt, mem).is_null();
            CloseClipboard();
            ok
        }
    }
    #[cfg(not(windows))]
    {
        let _ = html;
        false
    }
}

/// The fragment of the current "HTML Format" clipboard content, if any.
pub fn get_html() -> Option<String> {
    #[cfg(windows)]
    {
        let fmt = html_format_id();
        if fmt == 0 || unsafe { IsClipboardFormatAvailable(fmt) } == 0 {
            return None;
        }
        if !open_clipboard() {
            return None;
        }
        let result = unsafe {
            let handle = GetClipboardData(fmt);
            if handle.is_null() {
                CloseClipboard();
                return None;
            }
            let src = GlobalLock(handle) as *const u8;
            if src.is_null() {
                CloseClipboard();
                return None;
            }
            let cap = 1 << 20;
            let mut len = 0usize;
            while len < cap && *src.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf8_lossy(std::slice::from_raw_parts(src, len)).into_owned();
            GlobalUnlock(handle);
            text
        };
        unsafe { CloseClipboard() };
        parse_cf_html(&result)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn html_format_id() -> u32 {
    let name: Vec<u16> = "HTML Format\0".encode_utf16().collect();
    unsafe { RegisterClipboardFormatW(name.as_ptr()) }
}

/// CF_HTML payload: fixed-width zero-padded offsets keep the header length
/// constant, so byte offsets can be computed before formatting.
fn build_cf_html(fragment: &str) -> String {
    let pre = "<html><body>\r\n<!--StartFragment-->";
    let post = "<!--EndFragment-->\r\n</body>\r\n</html>";
    let header_len = "Version:0.9\r\n\
StartHTML:0000000000\r\n\
EndHTML:0000000000\r\n\
StartFragment:0000000000\r\n\
EndFragment:0000000000\r\n"
        .len();
    let start_fragment = header_len + pre.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + post.len();
    format!(
        "Version:0.9\r\n\
StartHTML:{header_len:010}\r\n\
EndHTML:{end_html:010}\r\n\
StartFragment:{start_fragment:010}\r\n\
EndFragment:{end_fragment:010}\r\n\
{pre}{fragment}{post}"
    )
}

fn extract_offset(payload: &str, key: &str) -> Option<usize> {
    let i = payload.find(key)? + key.len();
    let digits: String = payload[i..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Pull the fragment (between the offset markers) out of a CF_HTML payload.
fn parse_cf_html(payload: &str) -> Option<String> {
    let start = extract_offset(payload, "StartFragment:")?;
    let end = extract_offset(payload, "EndFragment:")?;
    let bytes = payload.as_bytes();
    if start > end || end > bytes.len() {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::MutexGuard;

    /// The clipboard is a global resource: cargo runs tests in parallel
    /// threads, so the real-clipboard tests take turns.
    static CLIP_LOCK: Mutex<()> = Mutex::new(());

    fn lock_clipboard() -> MutexGuard<'static, ()> {
        CLIP_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(windows)]
    #[test]
    fn set_then_get_roundtrips() {
        let _guard = lock_clipboard();
        let text = format!("amk-clipboard-{}", std::process::id());
        assert!(set_text(&text), "set_text failed");
        assert_eq!(get_text().as_deref(), Some(text.as_str()));
    }

    #[cfg(windows)]
    #[test]
    fn set_then_get_html_roundtrips() {
        let _guard = lock_clipboard();
        let html = format!("<b>amk {}</b>", std::process::id());
        assert!(set_html(&html), "set_html failed");
        assert_eq!(get_html().as_deref(), Some(html.as_str()));
    }

    #[test]
    fn cf_html_builder_and_parser_agree_on_offsets() {
        let payload = build_cf_html("<b>hi</b>");
        assert_eq!(parse_cf_html(&payload).as_deref(), Some("<b>hi</b>"));
        // Vietnamese text must survive the byte-offset math.
        let payload = build_cf_html("<i>xin chào</i>");
        assert_eq!(parse_cf_html(&payload).as_deref(), Some("<i>xin chào</i>"));
    }

    #[test]
    fn parser_rejects_broken_payloads() {
        assert_eq!(parse_cf_html("no markers here"), None);
        assert_eq!(
            parse_cf_html("StartFragment:0000000099EndFragment:0000000005"),
            None
        );
    }
}
