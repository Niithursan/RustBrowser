param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [ValidateSet('static','dynamic','report')][string]$Expected='report',
    [string]$Dumpbin=''
)
$ErrorActionPreference='Stop'
$exe=(Resolve-Path -LiteralPath $Executable).Path
if(-not $Dumpbin){
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if(-not(Test-Path -LiteralPath $vswhere)){throw 'MSVC Build Tools with vswhere and dumpbin are required for import inspection.'}
    $Dumpbin=& $vswhere -latest -products '*' -find 'VC/Tools/MSVC/**/bin/Hostx64/x64/dumpbin.exe' | Select-Object -First 1
}
if(-not $Dumpbin){throw 'MSVC x64 dumpbin was not found.'}
# /DEPENDENTS includes both ordinary and delay-loaded import descriptors. This
# does not inspect DLLs loaded by name at runtime or dependencies of other DLLs.
$lines=@(& $Dumpbin /DEPENDENTS $exe)
if($LASTEXITCODE -ne 0){throw 'PE import inspection failed.'}
$imports=@($lines | ForEach-Object {$_.Trim()} | Where-Object {$_ -match '^[A-Za-z0-9_.-]+\.dll$'} | Sort-Object -Unique)
if(-not $imports.Count){throw 'No DLL import descriptors found; do not infer static linking from an empty report.'}
$crt=@($imports | Where-Object {$_ -match '^(vcruntime|msvcp|msvcr|concrt|api-ms-win-crt-)|^ucrtbase\.dll$'})
if($Expected -eq 'static' -and $crt.Count){throw "Static candidate imports an external CRT: $($crt -join ', ')"}
if($Expected -eq 'dynamic' -and -not($crt -match '^vcruntime')){throw 'Dynamic control has no VCRUNTIME import.'}
[pscustomobject]@{executable=$exe;sha256=(Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $exe).Length;imports=$imports;crt_imports=$crt;expected=$Expected;scope='direct and delay imports only; not a missing-runtime host test'}
