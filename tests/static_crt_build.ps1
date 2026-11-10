param([Parameter(Mandatory=$true)][string]$OutputRoot)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
$base=Assert-PackageOutput $OutputRoot $repo
Push-Location $repo
$encoded=$env:CARGO_ENCODED_RUSTFLAGS;$plain=$env:RUSTFLAGS
try{
 if(git status --porcelain){throw 'Run the build-failure regression on a clean committed tree'}
 $env:CARGO_ENCODED_RUSTFLAGS='owner-test-encoded-sentinel'
 $env:RUSTFLAGS='owner-test-plain-sentinel'
 $run=Join-Path $base ('f-'+[guid]::NewGuid().ToString('N').Substring(0,8))
 $failed=$false
 try{& (Join-Path $repo 'validation/build-static-crt.ps1') -OutputRoot $run -Cargo (Join-Path $run 'missing-cargo.exe') | Out-Null}catch{$failed=$true}
 if(-not $failed -or -not(Test-Path -LiteralPath $run) -or @(Get-ChildItem -LiteralPath $run -Directory).Count -ne 1){throw 'Build failure was not reached after candidate setup'}
 if($env:CARGO_ENCODED_RUSTFLAGS -cne 'owner-test-encoded-sentinel' -or $env:RUSTFLAGS -cne 'owner-test-plain-sentinel'){throw 'Failed static build changed caller Rust flags'}
 if(Get-ChildItem -LiteralPath $run -Filter 'candidate.json' -File -Recurse){throw 'Failed build wrote a candidate manifest'}
 'Static build: failed command restores caller flags and emits no candidate manifest.'
}finally{$env:CARGO_ENCODED_RUSTFLAGS=$encoded;$env:RUSTFLAGS=$plain;Pop-Location}
