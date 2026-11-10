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

fn main() -> std::process::ExitCode {
    if std::env::args_os().len() == 2
        && std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--version"))
    {
        println!("Rust Browser {}", env!("CARGO_PKG_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }
    // Never print a panic payload: it can contain data supplied by a website.
    std::panic::set_hook(Box::new(|_| {
        eprintln!(
            "Rust Browser encountered an internal error. Restart and inspect local application diagnostics. No report was uploaded."
        );
    }));
    #[cfg(windows)]
    {
        // Retain the marker through WebView/profile teardown. Private/alternative
        // profiles remain allowed; the marker only makes installers wait.
        let running = native_installation::Running::mark();
        if running.is_ok() && app::run().is_ok() {
            return std::process::ExitCode::SUCCESS;
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "Browser startup failed. Check WebView2 Runtime, close any other normal window, and check profile access and disk space. Inspect local diagnostics when available."
        );
        #[cfg(not(debug_assertions))]
        // SAFETY: main thread, static strings and no untrusted window handle.
        unsafe {
            use windows::{
                Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
                core::w,
            };
            let _ = MessageBoxW(
                None,
                w!(
                    "Rust Browser could not start.\n\nCheck that Microsoft WebView2 Runtime is installed. Close any other normal window using this profile, then check profile access and free disk space.\n\nExisting browsing data is preserved. Local diagnostics may contain a startup event if the profile was opened."
                ),
                w!("Rust Browser startup"),
                MB_ICONERROR | MB_OK,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("Rust Browser currently supports Windows x64 only.");
    std::process::ExitCode::FAILURE
}
