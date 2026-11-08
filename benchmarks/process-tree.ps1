function Read-SharedText([string]$Path) {
    $stream = [IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try { $reader=[IO.StreamReader]::new($stream); return $reader.ReadToEnd() } finally { $stream.Dispose() }
}
function Sample-Tree([int]$RootId,$CpuState,[double]$Elapsed,[string]$Stage) {
    $all=Get-CimInstance Win32_Process
    $ids=[Collections.Generic.HashSet[int]]::new();[void]$ids.Add($RootId)
    do { $before=$ids.Count; foreach($item in $all) {if($ids.Contains([int]$item.ParentProcessId)){[void]$ids.Add([int]$item.ProcessId)}} } while($before -ne $ids.Count)
    $working=0L;$private=0L;$cpu=0.0;$alive=0;$unavailable=0
    foreach($processId in $ids) {
        try {
            $item=Get-Process -Id $processId -ErrorAction Stop
            $key="$processId/$($item.StartTime.ToUniversalTime().Ticks)"
            $total=$item.TotalProcessorTime.TotalSeconds
            # Explicit doubles avoid PowerShell choosing Math.Max's integer overload.
            if($CpuState.ContainsKey($key)){$cpu+=[math]::Max([double]0,[double]($total-$CpuState[$key]))}
            $CpuState[$key]=$total;$working+=$item.WorkingSet64;$private+=$item.PrivateMemorySize64;$alive++
        } catch { $unavailable++ }
    }
    [pscustomobject]@{seconds=[math]::Round($Elapsed,3);stage=$Stage;processes=$alive;unavailable=$unavailable;working_mib=[math]::Round($working/1MB,2);private_mib=[math]::Round($private/1MB,2);cpu_seconds=$cpu;ids=@($ids)}
}
