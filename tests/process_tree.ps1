$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '../benchmarks/process-tree.ps1')
function Require([bool]$Value,[string]$Message){if(-not $Value){throw $Message}}
function Record([int]$Id,[int]$Parent,[int]$Seconds){[pscustomobject]@{ProcessId=$Id;ParentProcessId=$Parent;CreationDate=$epoch.AddSeconds($Seconds)}}
$epoch=[datetime]::Parse('2026-11-09T00:00:00Z').ToUniversalTime()
$snapshot=@((Record 10 1 100),(Record 20 10 50),(Record 21 10 101),(Record 22 21 102),(Record 30 21 104),(Record 31 30 103),(Record 40 99 110))
$tree=@(Get-ProcessTreeIdentities $snapshot 10 $epoch.AddSeconds(100))
Require (($tree.id | Sort-Object) -join ',' -eq '10,21,22,30') 'Reused parent IDs included older unrelated processes or lost live descendants'
Require (@(Get-ProcessTreeIdentities $snapshot 10 $epoch.AddSeconds(99)).Count -eq 0) 'A reused root ID was accepted'
Require (@(Get-ProcessTreeIdentities $snapshot 999 $epoch).Count -eq 0) 'A missing root produced fabricated descendants'
Require (Same-ProcessStart $epoch $epoch.AddTicks(9)) 'WMI precision conversion rejected one process'
Require (-not (Same-ProcessStart $epoch $epoch.AddMilliseconds(1))) 'Different process generations were treated as identical'
$sample=[pscustomobject]@{seconds=1;stage='idle';processes=4;unavailable=0;working_mib=10;private_mib=9;cpu_seconds=0.25;identities=$tree;ids=@(10,21);secret='not exported'}
$public=Public-ResourceSample $sample
Require (@($public.PSObject.Properties.Name).Count -eq 7) 'Unexpected resource export schema'
$json=$public | ConvertTo-Json -Compress
Require ($json -notmatch 'identities|secret|"ids"') 'Internal process identifiers or unrelated fields leaked into export'
Require ($public.cpu_seconds -eq 0.25 -and $public.processes -eq 4) 'Numeric counters changed during export'
$current=Get-Process -Id $PID
$identity=[pscustomobject]@{id=$current.Id;created=$current.StartTime.ToUniversalTime()}
Require (@(Live-ObservedChildren @($identity) -1).Count -eq 1) 'Live matching process was lost during exit verification'
$reused=[pscustomobject]@{id=$current.Id;created=$identity.created.AddSeconds(-1)}
Require (@(Live-ObservedChildren @($reused) -1).Count -eq 0) 'Exit verification waited for a reused PID'
'Resource sampler: parent/root PID reuse, descendant identity, timestamp precision and numeric-only export passed.'
