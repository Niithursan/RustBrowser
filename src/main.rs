#[cfg(windows)]
mod app;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod library_worker;
#[cfg(windows)]
mod native_downloads;
#[cfg(windows)]
mod native_security;
#[cfg(all(windows, test))]
mod native_validation;
#[cfg(windows)]
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    app::run()?;
    #[cfg(not(windows))]
    return Err("Rust Browser 0.4 currently supports Windows only".into());
    #[cfg(windows)]
    Ok(())
}
