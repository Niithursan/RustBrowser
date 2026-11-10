# Manual interrupted-installation test. Not a CI gate (each case runs Setup several
# times). Everything happens under a disposable validation identity and folder:
# the production identity, a real installation and the real profile are never
# targeted. The real profile is only read, to prove it is unchanged.
#
# For each delay, Setup is started silently and its whole process tree (setup.exe
# and the setup .tmp child it starts) is killed after that many milliseconds.
# Scenarios: a fresh install, and an upgrade over an installed baseline.
param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$BaselineExecutable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][string]$Compiler,
    [Parameter(Mandatory=$true)][string]$SourceCommit,
    [int[]]$DelaysMilliseconds = @(100, 300, 500, 700, 900, 1100, 1300, 1500, 1800, 2200, 3000),
    [ValidateSet('fresh', 'upgrade')][string[]]$Scenarios = @('fresh', 'upgrade'),
    [string]$Cargo = 'cargo'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$base = [IO.Path]::GetFullPath($OutputRoot)
if ($base.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Keep test output outside the repository.' }
. (Join-Path $repo 'benchmarks/process-tree.ps1')

# Read-only fingerprint of the real profile and production registration.
function Real-State {
    $profile = Join-Path $env:LOCALAPPDATA 'RustBrowser\v0.1'
    $files = @()
    if (Test-Path -LiteralPath $profile) {
        $files = @(Get-ChildItem -LiteralPath $profile -File -Recurse -Force -ErrorAction SilentlyContinue |
            Sort-Object FullName | ForEach-Object { "$($_.FullName.Substring($profile.Length)):$($_.Length):$($_.LastWriteTimeUtc.Ticks)" })
    }
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Niithursan.RustBrowser_is1'
    $registered = if (Test-Path -LiteralPath $key) { (Get-ItemProperty -LiteralPath $key).DisplayVersion } else { 'none' }
    [pscustomobject]@{ files = $files.Count; digest = (($files -join '|').GetHashCode()); production_registration = $registered
        browser_running = @(Get-Process -Name 'rust-browser' -ErrorAction SilentlyContinue).Count }
}
$realBefore = Real-State

$version = ([Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $Executable).Path)).ProductVersion
$baselineVersion = ([Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $BaselineExecutable).Path)).ProductVersion
New-Item -ItemType Directory -Path $base -Force | Out-Null
$package = & (Join-Path $repo 'packaging/windows/build-installer.ps1') -Executable $Executable -OutputRoot $base -Version $version -SourceCommit $SourceCommit -Compiler $Compiler
$baseline = & (Join-Path $repo 'packaging/windows/package.ps1') -Executable $BaselineExecutable -OutputRoot $base -Version $baselineVersion -SourceCommit $SourceCommit
$run = Join-Path $base ('interrupted-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run | Out-Null
Set-Content -LiteralPath (Join-Path $run '.rust-browser-install-validation') -Value 'Owned disposable installer validation'
$identity = [guid]::NewGuid().ToString('N')
$app = Join-Path $run 'application'
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RustBrowser.Validation.' + $identity + '_is1'

function Compile-Fixture($Payload, [string]$Version, [string]$Suffix) {
    $directory = Join-Path $run $Suffix; New-Item -ItemType Directory -Path $directory | Out-Null
    & $Compiler '/Q' ("/DAppVersion=$Version") ("/DPayloadRoot=$Payload") ("/DIconFile=" + (Join-Path $repo 'packaging/windows/rust-browser.ico')) ("/DValidationId=$identity") ("/DValidationRoot=$app") ("/O$directory") (Join-Path $repo 'packaging/windows/installer.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Validation installer did not compile.' }
    Join-Path $directory "rust-browser-v$Version-windows-x64-setup.exe"
}
function Silent([string]$Log) { @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', ('/LOG="' + $Log + '"')) }
function Setup-Code([string]$Program, [string]$Step) {
    $process = Start-Process -FilePath $Program -ArgumentList (Silent (Join-Path $run "$Step.log")) -PassThru -WindowStyle Hidden
    $null = $process.Handle
    try { if (-not $process.WaitForExit(60000)) { throw "Setup timed out: $Step" }; $process.WaitForExit(); $process.ExitCode } finally { $process.Dispose() }
}
function Run-Setup([string]$Program, [string]$Step) {
    $code = Setup-Code $Program $Step
    if ($code -ne 0) { throw "Setup failed ($code): $Step" }
}
function Uninstall-Owned([string]$Step, [switch]$KeepLeftovers) {
    # Use the uninstaller Apps & Features would run (the registered one); fall back
    # to unins000.exe only when nothing is registered.
    $uninstaller = Join-Path $app 'uninstall/unins000.exe'
    if (Test-Path -LiteralPath $registry) {
        $registered = (Get-ItemProperty -LiteralPath $registry).UninstallString
        if ($registered) { $uninstaller = $registered.Trim('"') }
    }
    if (Test-Path -LiteralPath $uninstaller) {
        # A refused uninstall is a finding, not a harness failure: count it, then
        # remove this run's own folder below.
        if ((Setup-Code $uninstaller $Step) -ne 0) { $script:uninstallRefusals++; return (Remove-Owned) }
        $clock = [Diagnostics.Stopwatch]::StartNew()
        while (Test-Path -LiteralPath (Join-Path $app 'uninstall')) {
            # The uninstaller exits before its cleanup child removes this folder.
            if ($clock.Elapsed.TotalSeconds -gt 60) {
                # Record what the uninstall left, then let the next install show
                # whether the leftover blocks it.
                $left = @(Get-ChildItem -LiteralPath $app -Recurse -Force -File -ErrorAction SilentlyContinue | ForEach-Object { "$($_.FullName.Substring($app.Length + 1)) ($($_.Length) bytes)" }) -join ', '
                $script:uninstallLeftovers += "${Step}: $left"
                if ($KeepLeftovers) { return }
                break
            }
            Start-Sleep -Milliseconds 100
        }
        $script:slowestCleanup = [math]::Max($script:slowestCleanup, $clock.Elapsed.TotalSeconds)
    }
    Remove-Owned
}
# A killed first install can leave files without an uninstaller. They are owned by
# this validation run only.
function Remove-Owned {
    # The uninstaller's cleanup can remove the empty folder at the same moment.
    if ((Test-Path -LiteralPath $app) -and $app.StartsWith($run + '\')) { Remove-Item -LiteralPath $app -Recurse -Force -ErrorAction SilentlyContinue }
    if (Test-Path -LiteralPath $registry) { Remove-Item -LiteralPath $registry -Force }
    $menu = Join-Path ([Environment]::GetFolderPath('Programs')) "Rust Browser validation $identity"
    if (Test-Path -LiteralPath $menu) { Remove-Item -LiteralPath $menu -Recurse -Force }
}
function Hash-Profile {
    @('profile', 'downloads') | ForEach-Object { Get-ChildItem -LiteralPath (Join-Path $run $_) -File -Recurse } | Sort-Object FullName |
        ForEach-Object { ($_.FullName.Substring($run.Length)) + ':' + (Get-FileHash -LiteralPath $_.FullName).Hash }
}
function Launch-Version {
    $exe = Join-Path $app 'rust-browser.exe'
    if (-not (Test-Path -LiteralPath $exe)) { return 'missing' }
    $out = Join-Path $run ('version-' + [guid]::NewGuid().ToString('N') + '.txt')
    $process = Start-Process -FilePath $exe -ArgumentList '--version' -PassThru -WindowStyle Hidden -RedirectStandardOutput $out
    $null = $process.Handle
    try {
        if (-not $process.WaitForExit(30000)) { $process.Kill(); return 'hung' }
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { return "exit $($process.ExitCode)" }
        return (Get-Content -LiteralPath $out -Raw).Trim()
    } finally { $process.Dispose() }
}
# Phase from Setup's own log at the moment it was killed.
function Phase([string]$Log) {
    if (-not (Test-Path -LiteralPath $Log)) { return 'before setup log' }
    $text = Read-SharedText $Log
    if ($text -match 'Installation process succeeded') { return 'after copy' }
    $entries = ([regex]::Matches($text, '-- File entry --')).Count
    $done = ([regex]::Matches($text, 'Successfully installed the file')).Count
    if ($entries -eq 0) { return 'before copy' }
    "during copy ($done of $entries files)"
}
function Kill-Tree([System.Diagnostics.Process]$Root, [datetime]$Created) {
    $tree = @(Get-ProcessTreeIdentities (Get-CimInstance Win32_Process) $Root.Id $Created)
    foreach ($item in $tree) {
        try { $p = [Diagnostics.Process]::GetProcessById($item.id); if (Same-ProcessStart $p.StartTime $item.created) { $p.Kill() } } catch {}
    }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while (@(Live-ObservedChildren $tree $Root.Id).Count -gt 0 -or -not $Root.HasExited) {
        if ($clock.Elapsed.TotalSeconds -gt 30) { throw 'Killed Setup processes did not exit.' }
        Start-Sleep -Milliseconds 50
    }
    $tree.Count
}

$previousRoot = $env:RUST_BROWSER_INSTALL_FIXTURE; $previousStage = $env:RUST_BROWSER_INSTALL_STAGE
$results = @()
$script:uninstallRefusals = 0
$script:slowestCleanup = 0
$script:uninstallLeftovers = @()
$script:installAfterUninstall = @()
try {
    $env:RUST_BROWSER_INSTALL_FIXTURE = $run; $env:RUST_BROWSER_INSTALL_STAGE = 'prepare'
    & $Cargo test --locked --test installation_profile installer_profile_harness -- --ignored --test-threads=1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the representative profile.' }
    $profileBefore = (Hash-Profile) -join '|'
    $old = Compile-Fixture $baseline.payload $baselineVersion 'baseline-installer'
    $new = Compile-Fixture $package.payload $version 'candidate-installer'
    $index = 0
    foreach ($scenario in $Scenarios) {
        foreach ($delay in $DelaysMilliseconds) {
            $index++
            Uninstall-Owned "reset-$index" -KeepLeftovers
            if (Test-Path -LiteralPath $app) {
                # Something survived the uninstall. Does it block the next install?
                $code = Setup-Code $(if ($scenario -eq 'upgrade') { $old } else { $new }) "after-leftover-$index"
                $script:installAfterUninstall += "case ${index}: exit $code"
                Remove-Owned
            }
            if ($scenario -eq 'upgrade') { Run-Setup $old "baseline-$index" }
            $log = Join-Path $run "killed-$index.log"
            $process = Start-Process -FilePath $new -ArgumentList (Silent $log) -PassThru -WindowStyle Hidden
            $null = $process.Handle
            $created = $process.StartTime
            Start-Sleep -Milliseconds $delay
            $exitedFirst = $process.HasExited
            $killed = if ($exitedFirst) { 0 } else { Kill-Tree $process $created }
            $process.Dispose()
            $phase = if ($exitedFirst) { 'after setup exited' } else { Phase $log }
            $afterKill = Launch-Version
            $registered = if (Test-Path -LiteralPath $registry) { (Get-ItemProperty -LiteralPath $registry).DisplayVersion } else { 'none' }
            $profileKept = ((Hash-Profile) -join '|') -ceq $profileBefore
            # Running the same Setup again must repair whatever the kill left.
            $repairExit = Setup-Code $new "repair-$index"
            $repaired = Launch-Version
            $exact = (Test-Path -LiteralPath (Join-Path $app 'rust-browser.exe')) -and ((Get-FileHash -LiteralPath (Join-Path $app 'rust-browser.exe')).Hash -ceq (Get-FileHash -LiteralPath $Executable).Hash)
            $repairReason = if ($repairExit -ne 0) { ((Select-String -LiteralPath (Join-Path $run "repair-$index.log") -Pattern 'PrepareToInstall failed: (.*)' | Select-Object -First 1).Matches.Groups[1].Value) } else { '' }
            $results += [pscustomobject]@{
                scenario = $scenario; delay_ms = $delay; processes_killed = $killed; phase = $phase
                launch_after_kill = $afterKill; registered_after_kill = $registered; profile_unchanged = $profileKept
                repair_exit = $repairExit; repair_refusal = $repairReason
                launch_after_repair = $repaired; repaired_bytes_exact = $exact
                profile_unchanged_after_repair = (((Hash-Profile) -join '|') -ceq $profileBefore)
            }
        }
    }
    Uninstall-Owned 'final'
    $env:RUST_BROWSER_INSTALL_STAGE = 'verify'
    & $Cargo test --locked --test installation_profile installer_profile_harness -- --ignored --test-threads=1 | Out-Null
    $profileSemantics = $LASTEXITCODE -eq 0
} finally {
    $env:RUST_BROWSER_INSTALL_FIXTURE = $previousRoot; $env:RUST_BROWSER_INSTALL_STAGE = $previousStage
    try { Uninstall-Owned 'cleanup' } catch {}
}
$realAfter = Real-State
$report = [pscustomobject]@{
    version = $version; baseline_version = $baselineVersion; identity = "RustBrowser.Validation.$identity"
    real_profile_before = $realBefore; real_profile_after = $realAfter
    real_profile_unchanged = ($realBefore.files -eq $realAfter.files -and $realBefore.digest -eq $realAfter.digest -and $realBefore.production_registration -eq $realAfter.production_registration)
    profile_semantics_after_all_cases = $profileSemantics; uninstall_refusals = $script:uninstallRefusals
    uninstall_leftovers = $script:uninstallLeftovers; install_after_leftover = $script:installAfterUninstall
    slowest_uninstall_cleanup_seconds = [math]::Round($script:slowestCleanup, 1); cases = $results
}
$json = Join-Path $base 'interrupted-install.json'
[IO.File]::WriteAllText($json, ($report | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
$results | Format-Table scenario, delay_ms, processes_killed, phase, launch_after_kill, registered_after_kill, profile_unchanged, repair_exit, launch_after_repair, repaired_bytes_exact -AutoSize
"Uninstaller refusals during resets: $script:uninstallRefusals; slowest uninstall cleanup: $([math]::Round($script:slowestCleanup, 1)) s"
"Uninstall leftovers: $(if ($script:uninstallLeftovers) { $script:uninstallLeftovers -join ' | ' } else { 'none' })"
"Install over a leftover: $(if ($script:installAfterUninstall) { $script:installAfterUninstall -join ', ' } else { 'not needed' })"
"Real profile unchanged: $($report.real_profile_unchanged) ($($realBefore.files) files; production registration $($realBefore.production_registration))"
"Results: $json"
# Fail on any outcome that leaves the user worse off than re-running Setup can fix.
$problems = @()
foreach ($case in $results) {
    $name = "$($case.scenario) $($case.delay_ms) ms"
    if ($case.repair_exit -ne 0) { $problems += "${name}: re-running Setup was refused ($($case.repair_refusal))" }
    if (-not $case.repaired_bytes_exact) { $problems += "${name}: repaired executable differs" }
    if (-not $case.profile_unchanged -or -not $case.profile_unchanged_after_repair) { $problems += "${name}: browser profile changed" }
}
if ($script:uninstallLeftovers) { $problems += "uninstall left files: $($script:uninstallLeftovers -join ' | ')" }
if ($script:installAfterUninstall | Where-Object { $_ -notmatch 'exit 0$' }) { $problems += "install after uninstall refused: $($script:installAfterUninstall -join ', ')" }
if ($script:uninstallRefusals) { $problems += "$($script:uninstallRefusals) uninstall(s) refused" }
if (-not $profileSemantics) { $problems += 'profile semantics changed' }
if (-not $report.real_profile_unchanged) { $problems += 'the real profile or production registration changed' }
if ($problems) { throw ("Interrupted installation problems:`n" + ($problems -join "`n")) }
'Interrupted installation: every case was repaired by re-running Setup; no leftovers; profiles unchanged.'

