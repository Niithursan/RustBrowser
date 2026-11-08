param(
    [Parameter(Mandatory=$true)][string]$ServoExecutable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [ValidateSet('Local','Loopback','Https')][string]$Page='Local',
    [ValidateRange(1024,65535)][int]$Port=8770,
    [switch]$Wait
)
. (Join-Path $PSScriptRoot 'tools.ps1')
$executable=Assert-ServoExecutable $ServoExecutable
$run=New-ServoRun $OutputRoot
$profile=Join-Path $run 'profile'
New-Item -ItemType Directory -Path $profile | Out-Null
$url=switch($Page){
    'Local' { ([uri](Join-Path $PSScriptRoot 'fixture.html')).AbsoluteUri }
    'Loopback' { "http://127.0.0.1:$Port/" }
    'Https' { 'https://example.com/' }
}
$arguments=Get-ServoArguments $profile $url
$previousProfile=$env:RUST_BROWSER_PROFILE
$previousHosts=$env:HOST_FILE
try {
    $env:RUST_BROWSER_PROFILE=$null
    $env:HOST_FILE=$null
    $process=Start-Process -FilePath $executable -ArgumentList $arguments -WorkingDirectory (Split-Path -Parent $executable) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $run 'stdout.txt') -RedirectStandardError (Join-Path $run 'stderr.txt')
    $null=$process.Handle
} finally {
    $env:RUST_BROWSER_PROFILE=$previousProfile
    $env:HOST_FILE=$previousHosts
}
if($Wait){$process.WaitForExit();if($process.ExitCode -ne 0){throw 'Servo exited with an error. Inspect only the disposable run logs.'}}
[pscustomobject]@{process_id=$process.Id;run=$run;profile=$profile;page=$Page}
