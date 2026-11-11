#![cfg_attr(
    all(windows, not(debug_assertions), not(test)),
    windows_subsystem = "windows"
)]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod library_worker;
#[cfg(windows)]
mod native_activity;
#[cfg(windows)]
mod native_background;
#[cfg(windows)]
mod native_downloads;
#[cfg(windows)]
mod native_installation;
#[cfg(windows)]
mod native_popup;
#[cfg(windows)]
mod native_security;
#[cfg(all(windows, test))]
mod native_validation;
#[cfg(all(windows, test))]
mod servo_validation;
#[cfg(windows)]
mod ui;

/// Tell the user something in a dialog (release) or on stderr (debug and tests).
#[cfg(windows)]
fn tell(title: &str, message: &str, error: bool) {
    #[cfg(debug_assertions)]
    {
        let _ = (title, error);
        eprintln!("{message}");
    }
    #[cfg(not(debug_assertions))]
    {
        use windows::{
            Win32::UI::WindowsAndMessaging::{
                MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW,
            },
            core::HSTRING,
        };
        // SAFETY: main thread, owned strings, no window handle.
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(message),
                &HSTRING::from(title),
                if error {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                } | MB_OK,
            );
        }
    }
}

/// The raw command line exactly as Windows passed it, before any splitting.
#[cfg(windows)]
fn raw_command_line() -> Vec<u16> {
    // SAFETY: GetCommandLineW returns a process-owned, NUL-terminated string that
    // stays valid for the life of the process; it is only read here.
    unsafe {
        let line = windows::Win32::System::Environment::GetCommandLineW();
        if line.is_null() {
            return Vec::new();
        }
        line.as_wide().to_vec()
    }
}

fn main() -> std::process::ExitCode {
    // Never print a panic payload: it can contain data supplied by a website.
    std::panic::set_hook(Box::new(|_| {
        eprintln!(
            "Rust Browser encountered an internal error. Restart and inspect local application diagnostics. No report was uploaded."
        );
    }));
    #[cfg(windows)]
    let (private, links) = match rust_browser::launch::parse(&raw_command_line()) {
        Ok(rust_browser::launch::Launch::Version) => {
            println!("Rust Browser {}", env!("CARGO_PKG_VERSION"));
            return std::process::ExitCode::SUCCESS;
        }
        Ok(rust_browser::launch::Launch::Open { private, links, .. }) => (private, links),
        Err(error) => {
            tell(
                "Rust Browser",
                &format!("Rust Browser did not start. {error}\n\nNothing was opened or changed."),
                true,
            );
            return std::process::ExitCode::from(2);
        }
    };
    #[cfg(windows)]
    {
        // Retain the marker through WebView/profile teardown. Private/alternative
        // profiles remain allowed; the marker only makes installers wait.
        let running = native_installation::Running::mark();
        let failure = match running {
            Ok(_) => match app::run(private, links) {
                Ok(()) => return std::process::ExitCode::SUCCESS,
                Err(failure) => failure,
            },
            Err(_) => app::StartFailure::Other,
        };
        let in_use = failure == app::StartFailure::ProfileInUse;
        #[cfg(debug_assertions)]
        eprintln!(
            "{}",
            if in_use {
                "Rust Browser is already running with this profile. Use its window; a second normal window is not supported."
            } else {
                "Browser startup failed. Check WebView2 Runtime, profile access and disk space. Inspect local diagnostics when available."
            }
        );
        #[cfg(not(debug_assertions))]
        // SAFETY: main thread, static strings and no untrusted window handle.
        unsafe {
            use windows::{
                Win32::UI::WindowsAndMessaging::{
                    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW,
                },
                core::w,
            };
            let _ = if in_use {
                MessageBoxW(
                    None,
                    w!(
                        "Rust Browser is already running.\n\nSwitch to its window from the taskbar and press Ctrl+T for a new tab. A second normal window for the same profile is not supported; a private window can be opened with Ctrl+Shift+N.\n\nNothing was changed."
                    ),
                    w!("Rust Browser"),
                    MB_ICONINFORMATION | MB_OK,
                )
            } else {
                MessageBoxW(
                    None,
                    w!(
                        "Rust Browser could not start.\n\nCheck that Microsoft WebView2 Runtime is installed, then check profile access and free disk space.\n\nExisting browsing data is preserved. Local diagnostics may contain a startup event if the profile was opened."
                    ),
                    w!("Rust Browser startup"),
                    MB_ICONERROR | MB_OK,
                )
            };
        }
    }
    #[cfg(not(windows))]
    eprintln!("Rust Browser currently supports Windows x64 only.");
    std::process::ExitCode::FAILURE
}
