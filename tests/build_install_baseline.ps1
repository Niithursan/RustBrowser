param([Parameter(Mandatory=$true)][string]$OutputRoot,[string]$Cargo='cargo')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
$base=Assert-PackageOutput $OutputRoot $repo
$run=Join-Path $base ('baseline-v022-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
$archive=Join-Path $run 'source.zip'
$source=Join-Path $run 'source'
$commit='b4536e7063ba0860c64c7e1b0c6d2466df8313da'
git archive --format=zip ("--output=$archive") $commit
if($LASTEXITCODE -ne 0){throw 'Fetch the published v0.22.2 commit before building the upgrade fixture.'}
Expand-Archive -LiteralPath $archive -DestinationPath $source
$target=Join-Path $base 'target-v022'
Assert-PlainPath $target
$current=Join-Path $target 'release/rust-browser.exe'
$old=Join-Path $run 'baseline/rust-browser.exe'
New-Item -ItemType Directory -Path (Split-Path $old) | Out-Null
$timer=[Diagnostics.Stopwatch]::StartNew()
# A separate stable target avoids stale workspace/build-script fingerprints when
# compiling two archived versions with identical package names in one target.
& $Cargo build --locked --release --manifest-path (Join-Path $source 'Cargo.toml') --target-dir $target
if($LASTEXITCODE -ne 0){throw 'Pinned v0.22.2 upgrade fixture build failed.'}
Copy-Item -LiteralPath $current -Destination $old
if(([Diagnostics.FileVersionInfo]::GetVersionInfo($old)).ProductVersion -cne '0.22.2'){throw 'Wrong upgrade baseline version.'}
[pscustomobject]@{executable=$old;commit=$commit;sha256=(Get-FileHash -LiteralPath $old).Hash.ToLowerInvariant();build_seconds=[math]::Round($timer.Elapsed.TotalSeconds,3)}
