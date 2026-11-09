function Read-SharedText([string]$Path) {
    $stream = [IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try { $reader=[IO.StreamReader]::new($stream); return $reader.ReadToEnd() } finally { $stream.Dispose() }
}
function Same-ProcessStart([datetime]$Left,[datetime]$Right) {
    # WMI timestamps have microsecond precision; kernel FILETIME has 100 ns ticks.
    return [math]::Abs(($Left.ToUniversalTime()-$Right.ToUniversalTime()).Ticks) -le 10
}
function Get-ProcessTreeIdentities($Snapshot,[int]$RootId,[datetime]$RootCreated) {
    $records=@{}
    foreach($item in $Snapshot){if($item.CreationDate){$records[[int]$item.ProcessId]=$item}}
    $root=$records[$RootId]
    if(-not $root -or -not (Same-ProcessStart $root.CreationDate $RootCreated)){return}
    $found=@{};$found[$RootId]=$root
    do {
        $before=$found.Count
        foreach($item in $records.Values){
            $parent=$found[[int]$item.ParentProcessId]
            # A PID can refer to a new process while an older unrelated child's
            # ParentProcessId retains the number. Reject that impossible birth order.
            if($parent -and $item.CreationDate.ToUniversalTime() -ge $parent.CreationDate.ToUniversalTime()){
                $found[[int]$item.ProcessId]=$item
            }
        }
    } while($before -ne $found.Count)
    foreach($item in $found.Values){[pscustomobject]@{id=[int]$item.ProcessId;created=$item.CreationDate.ToUniversalTime()}}
}
function Public-ResourceSample($Sample) {
    # Explicit allowlist: Windows PowerShell's -ExcludeProperty without -Property
    # did not remove internal identifiers from the previous JSON output.
    $Sample | Select-Object seconds,stage,processes,unavailable,working_mib,private_mib,cpu_seconds
}
function Sample-Tree([int]$RootId,$CpuState,[double]$Elapsed,[string]$Stage,[datetime]$RootCreated) {
    $identities=@(Get-ProcessTreeIdentities (Get-CimInstance Win32_Process) $RootId $RootCreated)
    $working=0L;$private=0L;$cpu=0.0;$alive=0;$unavailable=0
    if($identities.Count -eq 0){$unavailable=1}
    foreach($identity in $identities) {
        try {
            $item=Get-Process -Id $identity.id -ErrorAction Stop
            $created=$item.StartTime.ToUniversalTime()
            if(-not (Same-ProcessStart $created $identity.created)){throw 'Process ID was reused'}
            $key="$($identity.id)/$($created.Ticks)"
            $total=$item.TotalProcessorTime.TotalSeconds
            if($CpuState.ContainsKey($key)){$cpu+=[math]::Max([double]0,[double]($total-$CpuState[$key]))}
            $CpuState[$key]=$total;$working+=$item.WorkingSet64;$private+=$item.PrivateMemorySize64;$alive++
        } catch { $unavailable++ }
    }
    [pscustomobject]@{seconds=[math]::Round($Elapsed,3);stage=$Stage;processes=$alive;unavailable=$unavailable;working_mib=[math]::Round($working/1MB,2);private_mib=[math]::Round($private/1MB,2);cpu_seconds=$cpu;identities=$identities}
}
function Live-ObservedChildren($Identities,[int]$RootId) {
    foreach($identity in $Identities){
        if($identity.id -eq $RootId){continue}
        $item=Get-Process -Id $identity.id -ErrorAction SilentlyContinue
        if($item){try{if(Same-ProcessStart ($item.StartTime.ToUniversalTime()) $identity.created){$identity}}catch{$identity}}
    }
}
