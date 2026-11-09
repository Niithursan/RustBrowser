param(
    [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$RootProcessId,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [ValidateRange(1,60)][int]$Minutes=30,
    [ValidateRange(0,3600)][int]$Seconds=0
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../benchmarks/process-tree.ps1')
$process=Get-Process -Id $RootProcessId -ErrorAction Stop
$null=$process.Handle
$created=$process.StartTime.ToUniversalTime()
$root=[IO.Path]::GetFullPath($OutputRoot)
if(Test-Path -LiteralPath $root){
    if(@(Get-ChildItem -LiteralPath $root -Force).Count -gt 0 -and -not(Test-Path -LiteralPath (Join-Path $root '.rust-browser-samples'))){throw 'Choose an empty or previously marked measurement directory, never a browser profile.'}
}
New-Item -ItemType Directory -Path $root -Force|Out-Null
Set-Content -LiteralPath (Join-Path $root '.rust-browser-samples') -Value 'Numeric resource samples only' -Encoding utf8
$run=Join-Path $root ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run|Out-Null
$cpu=Get-CimInstance Win32_Processor|Select-Object -First 1
$os=Get-CimInstance Win32_OperatingSystem
$hardware=@{cpu=$cpu.Name;logical_processors=$cpu.NumberOfLogicalProcessors;ram_gib=[math]::Round($os.TotalVisibleMemorySize/1MB,2);os=$os.Caption;os_version=$os.Version;architecture=$os.OSArchitecture}
$duration=if($Seconds){$Seconds}else{60*$Minutes}
$clock=[Diagnostics.Stopwatch]::StartNew();$state=@{};$count=0
try {
    while($clock.Elapsed.TotalSeconds -lt $duration -and -not $process.HasExited){
        $sample=Sample-Tree $RootProcessId $state $clock.Elapsed.TotalSeconds 'interactive' $created
        Public-ResourceSample $sample | Export-Csv -LiteralPath (Join-Path $run 'samples.csv') -NoTypeInformation -Append -Encoding utf8
        $count++
        Start-Sleep -Milliseconds 750
        $process.Refresh()
    }
} finally {
    [ordered]@{hardware=$hardware;requested_seconds=$duration;observed_seconds=[math]::Round($clock.Elapsed.TotalSeconds,3);samples=$count;process_exited=$process.HasExited;method='Root plus recursively discovered descendants; numeric samples only; no URLs, titles, bodies or command lines.'}|ConvertTo-Json -Depth 4|Set-Content -LiteralPath (Join-Path $run 'context.json') -Encoding utf8
}
Write-Output "Recorded $count numeric samples. The sampler does not close or terminate the browser."
