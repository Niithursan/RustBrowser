param(
    [Parameter(Mandatory=$true)][string]$ProbeExecutable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][string]$Label,
    [int[]]$TabCounts = @(1,5,10,20),
    [ValidateRange(1,10)][int]$Repeats = 1,
    [ValidateRange(15,180)][int]$SettleSeconds = 30,
    [ValidateRange(5,180)][int]$SampleSeconds = 10,
    [ValidateRange(0,3600)][int]$SoakSeconds = 120,
    [switch]$FreshOnly,
    [switch]$Churn,
    [ValidateRange(5,180)][int]$RecoverySeconds = 15
)
$ErrorActionPreference = 'Stop'
$executable = (Resolve-Path -LiteralPath $ProbeExecutable).Path
$output = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $output) {
    if (@(Get-ChildItem -LiteralPath $output -Force).Count -gt 0 -and -not (Test-Path -LiteralPath (Join-Path $output '.rust-browser-benchmark'))) {
        throw 'Choose an empty or previously marked benchmark output directory, never a browser profile.'
    }
}
New-Item -ItemType Directory -Path $output -Force | Out-Null
Set-Content -LiteralPath (Join-Path $output '.rust-browser-benchmark') -Value 'Disposable synthetic benchmark only' -Encoding utf8
foreach ($count in $TabCounts) { if ($count -lt 1 -or $count -gt 20) { throw 'Tab counts must be between 1 and 20.' } }

. (Join-Path $PSScriptRoot 'process-tree.ps1')
$cpu=Get-CimInstance Win32_Processor | Select-Object -First 1
$os=Get-CimInstance Win32_OperatingSystem
$hardware=[ordered]@{cpu=$cpu.Name;logical_processors=$cpu.NumberOfLogicalProcessors;ram_gib=[math]::Round($os.TotalVisibleMemorySize/1MB,2);os=$os.Caption;os_version=$os.Version;architecture=$os.OSArchitecture}
$results=[Collections.Generic.List[object]]::new()
foreach($count in $TabCounts) { foreach($repeat in 1..$Repeats) {
    $case=Join-Path $output ("tabs-$count-repeat-$repeat-"+[guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $case | Out-Null
    Set-Content -LiteralPath (Join-Path $case '.rust-browser-benchmark') -Value 'Synthetic benchmark' -Encoding utf8
    $modes=if($FreshOnly){@('fresh-profile')}else{@('fresh-profile','warm-profile')}
    foreach($mode in $modes) {
        $env:RUST_BROWSER_BENCH_ROOT=$case;$env:RUST_BROWSER_BENCH_TABS="$count"
        $env:RUST_BROWSER_BENCH_SETTLE="$SettleSeconds";$env:RUST_BROWSER_BENCH_SAMPLE="$SampleSeconds";$env:RUST_BROWSER_BENCH_SOAK="$SoakSeconds";$env:RUST_BROWSER_BENCH_RECOVERY="$RecoverySeconds"
        $env:RUST_BROWSER_BENCH_CHURN=if($Churn){'1'}else{'0'}
        $metricsPath=Join-Path $case 'metrics.json'
        if(Test-Path -LiteralPath $metricsPath){Remove-Item -LiteralPath $metricsPath}
        $start=Get-Date
        $process=Start-Process -FilePath $executable -ArgumentList @('windows_performance_probe','--ignored','--test-threads=1') -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $case "$mode.stdout.txt") -RedirectStandardError (Join-Path $case "$mode.stderr.txt")
        # Retain the native handle so Windows PowerShell can retrieve ExitCode after exit.
        $null=$process.Handle
        $clock=[Diagnostics.Stopwatch]::StartNew();$samples=[Collections.Generic.List[object]]::new();$cpuState=@{};$lastMetrics=$null;$treeIds=@()
        while(-not $process.HasExited) {
            if($clock.Elapsed.TotalSeconds -gt (360+$SoakSeconds)){throw 'Benchmark exceeded its timeout; inspect the test process and logs.'}
            if(Test-Path -LiteralPath $metricsPath){try{$lastMetrics=Read-SharedText $metricsPath | ConvertFrom-Json}catch{}}
            $stage=if($lastMetrics){$lastMetrics.stage}else{'startup'}
            $sample=Sample-Tree $process.Id $cpuState $clock.Elapsed.TotalSeconds $stage;$treeIds=$sample.ids
            $samples.Add(($sample | Select-Object -ExcludeProperty ids))
            # Preserve numeric evidence even if the probe later fails or is interrupted.
            $sample | Select-Object -ExcludeProperty ids | Export-Csv -LiteralPath (Join-Path $case "$mode.samples.csv") -Append -NoTypeInformation -Encoding utf8
            Start-Sleep -Milliseconds 750
            $process.Refresh()
        }
        $process.WaitForExit()
        if($process.ExitCode -ne 0){throw "Benchmark failed for $count tabs ($mode). Read logs under the explicit output directory."}
        $metrics=Read-SharedText $metricsPath | ConvertFrom-Json
        if($metrics.stage -ne 'done'){throw 'Benchmark did not complete all phases.'}
        # Avoid attributing a previous run's lingering engine processes to the next run.
        for($attempt=0;$attempt -lt 240;$attempt++) {
            $remaining=@($treeIds | Where-Object {$_ -ne $process.Id} | ForEach-Object {Get-Process -Id $_ -ErrorAction SilentlyContinue})
            if($remaining.Count -eq 0){break};Start-Sleep -Milliseconds 250
        }
        if($remaining.Count -gt 0){throw 'Previous WebView2 children remained alive; measurements would be incomplete.'}
        $results.Add([ordered]@{label=$Label;tabs=$count;mode=$mode;repeat=$repeat;metrics=$metrics;samples=@($samples.ToArray())})
        [ordered]@{hardware=$hardware;parameters=@{settle=$SettleSeconds;sample=$SampleSeconds;soak=$SoakSeconds;recovery=$RecoverySeconds;churn=$Churn.IsPresent};runs=@($results.ToArray())} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8
        Write-Output "$Label / $count tabs / ${mode}: usable $([math]::Round($metrics.first_usable_ms)) ms; run finished in $([math]::Round(((Get-Date)-$start).TotalSeconds)) s"
    }
} }
Remove-Item Env:RUST_BROWSER_BENCH_ROOT,Env:RUST_BROWSER_BENCH_TABS,Env:RUST_BROWSER_BENCH_SETTLE,Env:RUST_BROWSER_BENCH_SAMPLE,Env:RUST_BROWSER_BENCH_SOAK,Env:RUST_BROWSER_BENCH_RECOVERY,Env:RUST_BROWSER_BENCH_CHURN
