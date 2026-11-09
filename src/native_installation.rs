//! A lifetime marker for Setup/Uninstall, not a single-instance lock or IPC channel.
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::Threading::CreateMutexW,
    },
    core::HSTRING,
};

pub struct Running(HANDLE);
impl Running {
    pub fn mark() -> windows::core::Result<Self> {
        Self::named(&HSTRING::from("Local\\RustBrowser.ApplicationRunning"))
    }
    fn named(name: &HSTRING) -> windows::core::Result<Self> {
        // SAFETY: no custom security descriptor, no ownership/wait, valid string.
        // Each process retains a handle; normal/abnormal exit releases its reference.
        unsafe { CreateMutexW(None, false, name).map(Self) }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns the returned handle, closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

    #[test]
    fn marker_allows_multiple_instances_and_survives_until_last_handle_closes() {
        let name = HSTRING::from(format!(
            "Local\\RustBrowser.UnitTest.{}",
            std::process::id()
        ));
        let first = Running::named(&name).unwrap();
        let second = Running::named(&name).unwrap();
        drop(first);
        // SAFETY: test-owned unique name; close the extra probe handle immediately.
        unsafe {
            let probe = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &name).unwrap();
            CloseHandle(probe).unwrap();
        }
        drop(second);
        // SAFETY: no handle remains; the name must no longer exist.
        assert!(unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) }.is_err());
    }
}
