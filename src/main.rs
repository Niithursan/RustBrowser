#[cfg(windows)]
mod app;
#[cfg(windows)]
mod engine;
mod navigation;
#[cfg(windows)]
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    app::run()?;
    #[cfg(not(windows))]
    return Err("Rust Browser 0.1 currently supports Windows only".into());
    #[cfg(windows)]
    Ok(())
}
