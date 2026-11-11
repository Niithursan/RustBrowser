//! Plain-text clipboard for the address-field menu. Only CF_UNICODETEXT is read or
//! written; anything else on the clipboard is ignored. Reads are bounded.
//!
//! Test builds use an in-memory clipboard so fixtures never touch the user's.

/// Longest clipboard text read (characters); the caller cuts it further.
const MAX_READ: usize = 1 << 20;

#[cfg(test)]
thread_local! {
    pub(crate) static TEST_CLIPBOARD: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[cfg(test)]
pub fn write_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    TEST_CLIPBOARD.with(|clip| *clip.borrow_mut() = text.to_string());
    Ok(())
}

#[cfg(test)]
pub fn read_text() -> Result<String, Box<dyn std::error::Error>> {
    Ok(TEST_CLIPBOARD.with(|clip| clip.borrow().chars().take(MAX_READ).collect()))
}

#[cfg(not(test))]
use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        Ole::CF_UNICODETEXT,
    },
};

/// Holds the clipboard open and closes it once.
#[cfg(not(test))]
struct Open;
#[cfg(not(test))]
impl Open {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Another program may hold it briefly; try a few times.
        for _ in 0..10 {
            // SAFETY: no owner window; closed by Drop.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Err("The clipboard is in use by another program.".into())
    }
}
#[cfg(not(test))]
impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: the clipboard was opened by `Open::new`.
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

#[cfg(not(test))]
pub fn write_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let _open = Open::new()?;
    // SAFETY: the allocation is sized for `wide`, locked only while copying, and
    // owned by the clipboard after SetClipboardData succeeds (freed otherwise).
    unsafe {
        EmptyClipboard()?;
        let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
        let target = GlobalLock(memory) as *mut u16;
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err("Clipboard memory could not be locked.".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
        let _ = GlobalUnlock(memory);
        if SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))).is_err() {
            let _ = GlobalFree(Some(memory));
            return Err("Text could not be put on the clipboard.".into());
        }
    }
    Ok(())
}

#[cfg(not(test))]
pub fn read_text() -> Result<String, Box<dyn std::error::Error>> {
    let _open = Open::new()?;
    // SAFETY: the clipboard owns the handle; it is locked only while copying, and
    // reading stops at the first NUL or at the allocation's size.
    unsafe {
        if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).is_err() {
            return Ok(String::new());
        }
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0))?;
        let memory = HGLOBAL(handle.0);
        let size = GlobalSize(memory) / 2;
        let source = GlobalLock(memory) as *const u16;
        if source.is_null() {
            return Ok(String::new());
        }
        let units = std::slice::from_raw_parts(source, size.min(MAX_READ));
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..end]);
        let _ = GlobalUnlock(memory);
        Ok(text)
    }
}
