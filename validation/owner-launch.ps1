# Copied into an owner bundle as Launch.ps1; run only after reviewing owner.json.
param([switch]$Installed,[switch]$VerifyOnly)
$ErrorActionPreference='Stop'
$record=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'owner.json') -Raw | ConvertFrom-Json
if($record.schema -ne 1 -or $record.identity -cnotmatch '^RustBrowser\.Validation\.[a-f0-9]{32}$' -or $record.sha256 -cnotmatch '^[a-f0-9]{64}$' -or $record.source_commit -cnotmatch '^[a-f0-9]{40}$'){throw 'Invalid owner-bundle record.'}
. (Join-Path $PSScriptRoot 'paths.ps1')
$root=Assert-PackageOutput $PSScriptRoot $record.repository
$production=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs/RustBrowser')).TrimEnd('\')
if($root -eq $production -or $root.StartsWith($production+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Owner bundles must stay outside the production installation.'}
$profile=Join-Path $root 'profile'
Assert-PlainPath $profile
$marker=Join-Path $profile '.rust-browser-owner-profile'
if(Test-Path -LiteralPath $profile){
    if(-not(Test-Path -LiteralPath $marker) -or (Get-Content -LiteralPath $marker -Raw).Trim() -cne $record.identity){throw 'Refusing an existing profile without this bundle ownership marker.'}
}
$exe=Join-Path $root 'portable/rust-browser.exe'
if($Installed){
    $exe=Join-Path $root 'application/rust-browser.exe'
    $ini=Join-Path $root 'application/installation.ini'
    $identityPattern='^'+[regex]::Escape('Identity='+$record.identity)+'$'
    if(-not(Test-Path -LiteralPath $ini) -or -not(Select-String -LiteralPath $ini -Pattern $identityPattern -CaseSensitive -Quiet)){throw 'The validation installation identity does not match.'}
}
Assert-PlainPath $exe
if(-not(Test-Path -LiteralPath $exe -PathType Leaf) -or (Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -cne $record.sha256){throw 'Owner executable is missing or differs from the recorded candidate.'}
if($VerifyOnly){[pscustomobject]@{executable=$exe;profile=$profile;source_commit=$record.source_commit;sha256=$record.sha256;will_launch=$false};return}
New-Item -ItemType Directory -Path $profile -Force | Out-Null
if(-not(Test-Path -LiteralPath $marker)){[IO.File]::WriteAllText($marker,$record.identity,[Text.Encoding]::ASCII)}
$previous=$env:RUST_BROWSER_PROFILE
try{
    $env:RUST_BROWSER_PROFILE=$profile
    $process=Start-Process -FilePath $exe -WorkingDirectory $root -PassThru -WindowStyle Hidden
    [pscustomobject]@{process_id=$process.Id;profile=$profile;source_commit=$record.source_commit;sha256=$record.sha256}
}finally{$env:RUST_BROWSER_PROFILE=$previous}
