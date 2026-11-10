use std::{env, fs, io, path::PathBuf, process::Command};

fn resource_compiler() -> io::Result<PathBuf> {
    if let Some(bin) = env::var_os("WindowsSdkVerBinPath") {
        let candidate = PathBuf::from(bin).join("x64/rc.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let root = PathBuf::from(
        env::var_os("ProgramFiles(x86)")
            .ok_or_else(|| io::Error::other("Windows SDK is required"))?,
    )
    .join("Windows Kits/10/bin");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let version: Option<Vec<u32>> = name
            .to_string_lossy()
            .split('.')
            .map(|part| part.parse().ok())
            .collect();
        if let Some(version) = version
            && version.len() == 4
        {
            let path = entry.path().join("x64/rc.exe");
            if path.is_file() {
                candidates.push((version, path));
            }
        }
    }
    candidates
        .into_iter()
        .max_by_key(|(version, _)| version.clone())
        .map(|(_, path)| path)
        .ok_or_else(|| {
            io::Error::other(
                "Install the Windows 10/11 SDK resource compiler with Visual Studio C++ tools",
            )
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=packaging/windows/app.manifest");
    println!("cargo:rerun-if-changed=packaging/windows/rust-browser.ico");
    println!("cargo:rerun-if-env-changed=WindowsSdkVerBinPath");
    if env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }
    if env::var("CARGO_CFG_TARGET_ARCH")? != "x86_64" || env::var("CARGO_CFG_TARGET_ENV")? != "msvc"
    {
        return Err("The packaged Windows application currently requires x64 MSVC".into());
    }
    let version = env::var("CARGO_PKG_VERSION")?;
    let parts: Vec<u16> = ["MAJOR", "MINOR", "PATCH"]
        .into_iter()
        .map(|part| {
            env::var(format!("CARGO_PKG_VERSION_{part}"))?
                .parse()
                .map_err(Box::<dyn std::error::Error>::from)
        })
        .collect::<Result<_, _>>()?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo output directory is missing")?);
    let manifest = output.join("app.manifest");
    fs::write(
        &manifest,
        fs::read_to_string("packaging/windows/app.manifest")?.replace(
            "@VERSION@",
            &format!("{}.{}.{}.0", parts[0], parts[1], parts[2]),
        ),
    )?;
    let resource = output.join("application.rc");
    let result = output.join("application.res");
    let manifest = manifest.to_string_lossy().replace('\\', "\\\\");
    let icon = env::current_dir()?.join("packaging/windows/rust-browser.ico");
    let icon = icon.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        &resource,
        format!(
            r#"1 24 "{manifest}"
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {},{},{},0
PRODUCTVERSION {},{},{},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "FileDescription", "Rust Browser for Windows\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "Rust Browser\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "InternalName", "rust-browser\0"
   VALUE "OriginalFilename", "rust-browser.exe\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0409, 1200
 END
END
"#,
            parts[0], parts[1], parts[2], parts[0], parts[1], parts[2]
        ),
    )?;
    if !Command::new(resource_compiler()?)
        .arg("/nologo")
        .arg("/fo")
        .arg(&result)
        .arg(&resource)
        .status()?
        .success()
    {
        return Err("Windows resource compilation failed".into());
    }
    println!("cargo:rustc-link-arg-bin=rust-browser={}", result.display());
    println!("cargo:rustc-link-arg-bin=rust-browser=/MANIFEST:NO");
    Ok(())
}
