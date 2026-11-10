param(
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [string]$Cargo='cargo'
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
$base=Assert-PackageOutput $OutputRoot $repo
Push-Location -LiteralPath $repo
$oldFlags=$env:CARGO_ENCODED_RUSTFLAGS
$oldPlain=$env:RUSTFLAGS
try{
    if(git status --porcelain){throw 'Commit the candidate sources before building a qualified artifact.'}
    $commit=git rev-parse HEAD
    if($LASTEXITCODE -ne 0){throw 'Cannot identify the source commit.'}
    $run=Join-Path $base ('s-'+[guid]::NewGuid().ToString('N').Substring(0,8))
    New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
    # Explicit target keeps host proc macros/build scripts out of target CRT flags.
    # This invocation is opt-in. No .cargo config or production workflow changes.
    $env:CARGO_ENCODED_RUSTFLAGS='-C'+[char]31+'target-feature=+crt-static'
    $env:RUSTFLAGS=$null
    $timer=[Diagnostics.Stopwatch]::StartNew()
    & $Cargo build --locked --release --target x86_64-pc-windows-msvc --target-dir (Join-Path $run 'target')
    if($LASTEXITCODE -ne 0){throw 'Static CRT build failed; retained output is not qualified.'}
    $seconds=$timer.Elapsed.TotalSeconds
    $exe=Join-Path $run 'target/x86_64-pc-windows-msvc/release/rust-browser.exe'
    $imports=& (Join-Path $PSScriptRoot 'check-crt.ps1') -Executable $exe -Expected static
    $record=[ordered]@{schema=1;source_commit=$commit;target='x86_64-pc-windows-msvc';crt='static-experiment';rustflags=@('-C','target-feature=+crt-static');build_seconds=$seconds;executable=$exe;sha256=$imports.sha256;bytes=$imports.bytes;imports=$imports.imports;qualified=$false;publication_authorized=$false}
    $manifest=Join-Path $run 'candidate.json'
    [IO.File]::WriteAllText($manifest,($record | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
    [pscustomobject]@{executable=$exe;manifest=$manifest;source_commit=$commit;sha256=$imports.sha256;build_seconds=$seconds}
}finally{
    $env:CARGO_ENCODED_RUSTFLAGS=$oldFlags
    $env:RUSTFLAGS=$oldPlain
    Pop-Location
}
