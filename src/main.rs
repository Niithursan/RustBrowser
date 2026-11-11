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
mod native_clipboard;
#[cfg(windows)]
mod native_downloads;
#[cfg(windows)]
mod native_extensions;
#[cfg(windows)]
mod native_handoff;
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

// On Windows every path returns from the platform block; the fallback is for others.
#[allow(unreachable_code)]
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
        if running.is_err() {
            return startup_failed();
        }
        return start_or_hand_off(private, links);
    }
    #[cfg(not(windows))]
    eprintln!("Rust Browser currently supports Windows x64 only.");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn startup_failed() -> std::process::ExitCode {
    tell(
        "Rust Browser startup",
        "Rust Browser could not start.\n\nCheck that Microsoft WebView2 Runtime is installed, then check profile access and free disk space.\n\nExisting browsing data is preserved. Local diagnostics may contain a startup event if the profile was opened.",
        true,
    );
    std::process::ExitCode::FAILURE
}

/// Start the browser, or hand the links to the normal window that already holds
/// this profile. While that window is starting or closing nothing answers: keep
/// trying for a few seconds, taking over the profile if it becomes free. Never
/// open a second window on the same profile, and always say what happened.
#[cfg(windows)]
fn start_or_hand_off(private: bool, links: Vec<String>) -> std::process::ExitCode {
    use native_handoff::Outcome;
    use rust_browser::handoff::{Request, Status};
    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(6);
    let started = std::time::Instant::now();
    let request = if links.is_empty() {
        Request::NewWindow
    } else {
        Request::Open(links.clone())
    };
    let endpoint = rust_browser::storage::profile_dir()
        .ok()
        .and_then(|profile| native_handoff::endpoint(&profile));
    loop {
        match app::run(private, links.clone()) {
            Ok(()) => return std::process::ExitCode::SUCCESS,
            Err(app::StartFailure::ProfileInUse) => {}
            Err(app::StartFailure::Other) => return startup_failed(),
        }
        let outcome = endpoint
            .as_deref()
            .map_or(Outcome::Failed, |name| native_handoff::send(name, &request));
        let message = match outcome {
            Outcome::Answered(Status::Accepted) => return std::process::ExitCode::SUCCESS,
            Outcome::NoServer | Outcome::Answered(Status::Closing)
                if started.elapsed() < PATIENCE =>
            {
                std::thread::sleep(std::time::Duration::from_millis(300));
                continue;
            }
            Outcome::NoServer | Outcome::Answered(Status::Closing) => {
                "Rust Browser is already running with this profile but did not accept the link. It may be starting, closing or not responding. Wait a moment and try again, or close it from Task Manager if it stays unresponsive."
            }
            Outcome::Answered(Status::Refused) => {
                "The running Rust Browser refused the request: the link is not one it can open."
            }
            Outcome::Answered(Status::Busy) => {
                "The running Rust Browser is busy (still starting, clearing browsing data, or receiving many links at once). Try again in a few seconds."
            }
            Outcome::Answered(Status::NotResponding) | Outcome::NotResponding => {
                "The running Rust Browser is not responding, so the link was not opened. Wait, or close it from Task Manager and try again."
            }
            Outcome::Untrusted => {
                "Another program is using Rust Browser's link channel, so nothing was sent to it. Close other programs or copies of Rust Browser that use this profile and try again."
            }
            Outcome::Failed => "Rust Browser could not hand the link to its running window.",
        };
        tell(
            "Rust Browser",
            &format!("{message}\n\nNo second window was opened and nothing was changed."),
            true,
        );
        return std::process::ExitCode::from(3);
    }
}
