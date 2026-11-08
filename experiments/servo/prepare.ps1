param(
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [string]$Archive
)
. (Join-Path $PSScriptRoot 'tools.ps1')
$pin=Get-ServoPin
$run=New-ServoRun $OutputRoot
if($Archive){$archivePath=(Resolve-Path -LiteralPath $Archive).Path}else{
    $archivePath=Join-Path $run 'servo.zip'
    $ProgressPreference='SilentlyContinue'
    Invoke-WebRequest -UseBasicParsing -Uri $pin.archive_url -OutFile $archivePath
}
if((Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pin.archive_sha256){throw 'Servo archive digest mismatch; nothing was executed.'}
$package=Join-Path $run 'package'
Expand-Archive -LiteralPath $archivePath -DestinationPath $package
$executable=Assert-ServoExecutable (Join-Path $package $pin.executable)
[pscustomobject]@{executable=$executable;source_revision=$pin.source_revision;archive_sha256=$pin.archive_sha256;run=$run}
