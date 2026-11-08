param(
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [ValidateSet('AppContainer','LPAC')][string]$Mode = 'AppContainer',
    [ValidateRange(1,5)][int]$Repeats = 1,
    [switch]$Diagnostics
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../tools.ps1')

# Only change ACLs on newly allocated files outside the repo and real profiles.
$root = New-ServoRun $OutputRoot
$build = Join-Path $root 'build'
New-Item -ItemType Directory -Path $build | Out-Null
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Install Visual Studio C++ tools and the Windows SDK.' }
    $installation = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'Visual Studio C++ tools were not found.' }
    $setup = Join-Path $installation 'VC/Auxiliary/Build/vcvars64.bat'
    $environment = & cmd.exe /d /c ('"' + $setup + '" >nul && set')
    if ($LASTEXITCODE -ne 0) { throw 'Windows SDK environment setup failed.' }
    foreach ($line in $environment) {
        if ($line -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
        }
    }
}

$flags = @('/nologo','/std:c++17','/W4','/WX','/EHsc','/MT',
    '/DUNICODE','/D_UNICODE','/D_WIN32_WINNT=0x0A00')
& cl.exe @flags (Join-Path $PSScriptRoot 'boundary_probe.cpp') "/Fo$(Join-Path $build probe.obj)" "/Fe$(Join-Path $build boundary-probe.exe)"
if ($LASTEXITCODE -ne 0) { throw 'Boundary broker compilation failed.' }
& cl.exe @flags (Join-Path $PSScriptRoot 'boundary_child.cpp') "/Fo$(Join-Path $build child.obj)" "/Fe$(Join-Path $build boundary-child.exe)" /link /SUBSYSTEM:WINDOWS
if ($LASTEXITCODE -ne 0) { throw 'Boundary child compilation failed.' }
if ($Diagnostics) {
    & cl.exe @flags (Join-Path $PSScriptRoot 'wire_tests.cpp') "/Fo$(Join-Path $build wire.obj)" "/Fe$(Join-Path $build wire-tests.exe)"
    if ($LASTEXITCODE -ne 0) { throw 'Protocol test compilation failed.' }
    & (Join-Path $build 'wire-tests.exe')
    if ($LASTEXITCODE -ne 0) { throw 'Protocol regression tests failed.' }
}

function New-OwnedProbeRun {
    $run = Join-Path $root ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    Set-Content -LiteralPath (Join-Path $run '.rust-browser-isolation-probe') -Value 'Synthetic objects only'
    Copy-Item -LiteralPath (Join-Path $build 'boundary-probe.exe'),(Join-Path $build 'boundary-child.exe') -Destination $run
    return $run
}

function Invoke-OwnedProbe([string]$Run, [string]$ProbeMode) {
    $process = Start-Process -FilePath (Join-Path $run 'boundary-probe.exe') -ArgumentList ('--' + $ProbeMode) `
        -WorkingDirectory $run -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $run 'result.txt') -RedirectStandardError (Join-Path $run 'failure.txt')
    $null = $process.Handle
    try {
        if (-not $process.WaitForExit(45000)) {
            Stop-Process -InputObject $process -ErrorAction SilentlyContinue
            throw "Owned probe exceeded its deadline. Inspect $run for cleanup status."
        }
        $process.WaitForExit()
        return [pscustomobject]@{
            exit_code = $process.ExitCode
            stdout = Get-Content -LiteralPath (Join-Path $run 'result.txt')
            stderr = Get-Content -LiteralPath (Join-Path $run 'failure.txt')
        }
    } finally {
        $process.Dispose()
    }
}
foreach ($attempt in 1..$Repeats) {
    $run = New-OwnedProbeRun
    $probeMode = $Mode.ToLowerInvariant()
    if ($Diagnostics) { $probeMode = 'diagnostics-' + $probeMode }
    $result = Invoke-OwnedProbe $run $probeMode
    $result.stdout
    $result.stderr
    if ($result.exit_code -ne 0) {
        throw "The $Mode probe failed with exit $($result.exit_code). Inspect $run."
    }
}
if ($Diagnostics) {
    foreach ($fault in 'malformed','hang') {
        $run = New-OwnedProbeRun
        $result = Invoke-OwnedProbe $run ('diagnostics-' + $fault)
        $result.stdout
        $result.stderr
        if ($result.exit_code -ne 0) { throw "The $fault IPC rejection test failed. Inspect $run." }
    }
}

$run = New-OwnedProbeRun
$crashMode = 'crash'
if ($Diagnostics) {
    $crashMode = if ($Mode -eq 'LPAC') { 'diagnostics-crash-lpac' } else { 'diagnostics-crash' }
}
$crash = Invoke-OwnedProbe $run $crashMode
if ($crash.exit_code -ne 74) { throw "Broker crash case failed. Inspect $run." }
$identity = Get-Content -LiteralPath (Join-Path $run 'child-identity.txt') -Raw
if ($identity -notmatch '^(\d+) (\d+)$') { throw 'Invalid owned child identity.' }
$childId = [int]$matches[1]
$created = [long]$matches[2]
Start-Sleep -Seconds 2
$child = Get-Process -Id $childId -ErrorAction SilentlyContinue
if ($child -and $child.StartTime.ToUniversalTime().ToFileTimeUtc() -eq $created) {
    throw "The owned child survived broker termination. Inspect $run."
}
$cleanup = Invoke-OwnedProbe $run 'cleanup'
$cleanup.stdout
$cleanup.stderr
if ($cleanup.exit_code -ne 0 -or (Test-Path -LiteralPath (Join-Path $run 'profile-created.marker'))) {
    throw "Owned profile recovery failed. Inspect $run."
}
'broker_crash_child_exit_and_profile_recovery=passed'
"Completed $Repeats $Mode probe run(s). Scratch sentinels are retained."
