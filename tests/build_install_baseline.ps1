param([Parameter(Mandatory=$true)][string]$OutputRoot,[string]$Cargo='cargo')
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
$base=Assert-PackageOutput $OutputRoot $repo
$run=Join-Path $base ('baseline-v013-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
$archive=Join-Path $run 'source.zip'
$source=Join-Path $run 'source'
$commit='ca7ac142031569279f1e1be0beffb53473af8cf7'
git archive --format=zip ("--output=$archive") $commit
if($LASTEXITCODE -ne 0){throw 'Fetch the verified v0.13 commit before building the upgrade fixture.'}
Expand-Archive -LiteralPath $archive -DestinationPath $source
$target=Join-Path $base 'target-v013'
Assert-PlainPath $target
$current=Join-Path $target 'release/rust-browser.exe'
$old=Join-Path $run 'baseline/rust-browser.exe'
New-Item -ItemType Directory -Path (Split-Path $old) | Out-Null
$timer=[Diagnostics.Stopwatch]::StartNew()
# A separate stable target avoids stale workspace/build-script fingerprints when
# compiling two archived versions with identical package names in one target.
& $Cargo build --locked --release --manifest-path (Join-Path $source 'Cargo.toml') --target-dir $target
if($LASTEXITCODE -ne 0){throw 'Pinned v0.13 upgrade fixture build failed.'}
Copy-Item -LiteralPath $current -Destination $old
if(([Diagnostics.FileVersionInfo]::GetVersionInfo($old)).ProductVersion -cne '0.13.0'){throw 'Wrong upgrade baseline version.'}
[pscustomobject]@{executable=$old;commit=$commit;sha256=(Get-FileHash -LiteralPath $old).Hash.ToLowerInvariant();build_seconds=[math]::Round($timer.Elapsed.TotalSeconds,3)}
