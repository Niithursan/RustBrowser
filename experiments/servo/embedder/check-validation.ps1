param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [ValidateRange(1,5)][int]$Repeats = 3
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../tools.ps1')
$exe = (Resolve-Path -LiteralPath $Executable).Path
if ((Split-Path $exe -Leaf) -ne 'rust_browser_embedder.exe') {
    throw 'Supply the pinned standalone embedder from SOURCE_BUILD.md.'
}
foreach ($attempt in 1..$Repeats) {
    $run = New-ServoRun $OutputRoot
    $p = Start-Process -FilePath $exe -ArgumentList '--validate' -WorkingDirectory $run -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $run 'result.txt') -RedirectStandardError (Join-Path $run 'errors.txt')
    $null = $p.Handle
    try {
        if (-not $p.WaitForExit(45000)) {
            Stop-Process -InputObject $p -ErrorAction SilentlyContinue
            throw "Owned validation exceeded its deadline. Inspect $run."
        }
        $p.WaitForExit()
        $lines = Get-Content -LiteralPath (Join-Path $run 'result.txt')
        $reports = @($lines | Where-Object { $_ -like 'servo scale=*' })
        if ($p.ExitCode -ne 0 -or $reports.Count -ne 4 -or $lines -notcontains 'teardown_complete') {
            throw "Servo fixture or teardown failed. Inspect $run."
        }
        "attempt=$attempt"
        $reports
    } finally {
        $p.Dispose()
    }
}
