param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [ValidateRange(1,10)][int]$Repeats = 3
)

. (Join-Path $PSScriptRoot '../tools.ps1')

$executablePath = (Resolve-Path -LiteralPath $Executable).Path
if ((Split-Path $executablePath -Leaf) -ne 'rust_browser_embedder.exe') {
    throw 'Supply the standalone embedder built from the pinned source procedure.'
}

function Invoke-OwnedSmoke([string]$Run, [string]$LogPrefix) {
    $launch = @{
        FilePath = $executablePath
        ArgumentList = '--smoke'
        WorkingDirectory = $Run
        WindowStyle = 'Normal'
        PassThru = $true
        RedirectStandardOutput = Join-Path $Run "$LogPrefix.stdout.txt"
        RedirectStandardError = Join-Path $Run "$LogPrefix.stderr.txt"
    }
    $process = Start-Process @launch
    $null = $process.Handle
    try {
        if (-not $process.WaitForExit(45000)) {
            Stop-Process -InputObject $process -ErrorAction SilentlyContinue
            throw 'The owned embedder exceeded its shutdown deadline.'
        }
        $process.WaitForExit()
        return $process.ExitCode
    } finally {
        $process.Dispose()
    }
}

$runs = foreach ($attempt in 1..$Repeats) {
    $run = New-ServoRun $OutputRoot
    $exitCode = Invoke-OwnedSmoke $run 'smoke'
    $log = Get-Content -LiteralPath (Join-Path $run 'smoke.stdout.txt')
    $present = $log | Select-String '^first_ready_present_ms=(\d+)$'
    $scale = $log | Select-String '^scale_factor=(\d+(?:\.\d+)?)$'
    if ($exitCode -ne 0 -or -not $present -or -not $scale -or $log -notcontains 'teardown_complete') {
        throw "Fixture presentation or shutdown failed. Inspect $run."
    }
    [pscustomobject]@{
        attempt = $attempt
        first_ready_present_ms = [int]$present.Matches[0].Groups[1].Value
        scale_factor = [double]::Parse($scale.Matches[0].Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
        exit_code = $exitCode
        retained_profile_files = @(Get-ChildItem -LiteralPath (Join-Path $run 'profile') -File -Recurse).Count
    }
}

# The last run is ours. Reusing its directory must fail before fixture or profile writes.
$before = Get-FileHash -LiteralPath (Join-Path $run 'fixture.html')
$refusalCode = Invoke-OwnedSmoke $run 'reuse-refusal'
$after = Get-FileHash -LiteralPath (Join-Path $run 'fixture.html')
if ($refusalCode -eq 0 -or $before.Hash -ne $after.Hash) {
    throw 'The embedder did not preserve the existing run on profile reuse.'
}

[pscustomobject]@{
    executable_sha256 = (Get-FileHash -LiteralPath $executablePath).Hash.ToLowerInvariant()
    runs = @($runs)
    reused_profile_refused = $true
}
