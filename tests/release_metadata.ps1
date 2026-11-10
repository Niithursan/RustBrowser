param(
    [Parameter(Mandatory=$true)][string]$MetadataPath,
    [Parameter(Mandatory=$true)][string]$OutputRoot
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
$OutputRoot=Assert-PackageOutput $OutputRoot $repo
$verify=Join-Path $repo 'packaging/windows/verify-release.ps1'
$source=Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
$run=Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ('metadata-validation-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
$metadata=Join-Path $run 'release.json'
foreach($asset in $source.assets){Copy-Item -LiteralPath (Join-Path (Split-Path $MetadataPath) $asset.name) -Destination (Join-Path $run $asset.name)}
$original=Get-Content -LiteralPath $MetadataPath -Raw
$script:count=0
function Write-Record($Record){[IO.File]::WriteAllText($metadata,($Record | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))}
function Refused([scriptblock]$Action){
    $rejected=$false;try{& $Action | Out-Null}catch{$rejected=$true}
    if(-not $rejected){throw 'Invalid release input was accepted.'};$script:count++
}
function Verify { & $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion '0.0.0' }
Write-Record $source
$result=Verify
if(-not $result.IntegrityVerified -or $result.PublisherAuthenticated -or $result.MetadataPinMatched){throw 'Unsigned verification made an authenticity claim.'}
$pin=(Get-FileHash -LiteralPath $metadata).Hash
$result=& $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion '0.0.0' -ExpectedMetadataSha256 $pin
if(-not $result.MetadataPinMatched -or $result.PublisherAuthenticated){throw 'Metadata pin reported publisher authentication.'}
Refused {& $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion '0.0.0' -ExpectedMetadataSha256 ('0'*64)}
foreach($bad in @('01.2.3','1.2','1.2.3.4','1.2.3-beta','65536.0.0','9999999999999999999.0.0','../escape','1.2.3 ',"1.2.3`n","1.2.3`r`n")){
    Refused {& $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion $bad}
}
Refused {& $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion $source.version}
$null=& $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion $source.version -AllowSameVersion
Refused {& $verify -MetadataPath $metadata -ArtifactDirectory $run -CurrentVersion '999.0.0'}
foreach($mutation in @(
    {param($r) $r.schema=2},
    {param($r) $r.schema='1'},
    {param($r) $r.profile_session='1'},
    {param($r) $r.assets[0].bytes=[double]$r.assets[0].bytes+0.5},
    {param($r) $r.signed=$true},
    {param($r) $r.signed='false'},
    {param($r) $r.version='01.2.3'},
    {param($r) $r.source_commit='untrusted'},
    {param($r) $r.architecture='arm64'},
    {param($r) $r.profile_library=999},
    {param($r) $r | Add-Member NoteProperty automatic_execute 'malware.exe'},
    {param($r) $r.assets[0].name='../escape.zip'},
    {param($r) $r.assets[0].name='https://example.com/file.zip'},
    {param($r) $r.assets[0].name=$r.assets[1].name},
    {param($r) $r.assets=@($r.assets[0])},
    {param($r) $r.assets[0].sha256='0'*64},
    {param($r) $r.assets[0].bytes++}
)){
    $record=$original | ConvertFrom-Json;& $mutation $record;Write-Record $record
    Refused {Verify}
}
Write-Record $source
foreach($asset in $source.assets){
    $path=Join-Path $run $asset.name
    try{
        [IO.File]::WriteAllBytes($path,[byte[]]@(0,1,2))
        Refused {Verify}
        Remove-Item -LiteralPath $path
        Refused {Verify}
    }finally{Copy-Item -LiteralPath (Join-Path (Split-Path $MetadataPath) $asset.name) -Destination $path}
}
[IO.File]::WriteAllText($metadata,('{"schema":'+(' '*65536)))
Refused {Verify}
[IO.File]::WriteAllText($metadata,'{"schema":')
Refused {Verify}
Write-Record $source
$null=Verify
"Release metadata: positive integrity/pin/explicit repair and $script:count malformed/downgrade/tampered/missing input refusals passed; no execution or authentication claim."
