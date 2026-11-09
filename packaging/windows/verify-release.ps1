param(
    [Parameter(Mandatory=$true)][string]$MetadataPath,
    [Parameter(Mandatory=$true)][string]$ArtifactDirectory,
    [Parameter(Mandatory=$true)][string]$CurrentVersion,
    [string]$ExpectedMetadataSha256 = '',
    [switch]$AllowSameVersion
)
$ErrorActionPreference='Stop'
function Parse-Version([string]$Value) {
    if($Value -cnotmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$'){throw 'Use a canonical stable X.Y.Z version.'}
    foreach($part in $Value.Split('.')){if($part.Length -gt 5 -or [int]$part -gt 65535){throw 'Version exceeds Windows metadata limits.'}}
    [version]$Value
}
function Assert-Ordinary([string]$Path,[switch]$Directory) {
    $item=Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if($Directory.IsPresent -ne $item.PSIsContainer){throw 'Unexpected artifact file type.'}
    $ancestor=$item.FullName
    while($ancestor){
        if((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Artifact paths must not traverse reparse points.'}
        $parent=Split-Path $ancestor -Parent;if($parent -eq $ancestor){break};$ancestor=$parent
    }
    $item
}
function Assert-Keys($Value,[string[]]$Expected) {
    $actual=@($Value.PSObject.Properties.Name | Sort-Object)
    if(($actual -join '|') -cne (($Expected | Sort-Object) -join '|')){throw 'Unexpected or missing release metadata fields.'}
}
$current=Parse-Version $CurrentVersion
$metadata=Assert-Ordinary $MetadataPath
$directory=Assert-Ordinary $ArtifactDirectory -Directory
if($metadata.Length -gt 65536){throw 'Release metadata is oversized.'}
$pinMatched=$false
if($ExpectedMetadataSha256){
    if($ExpectedMetadataSha256 -cnotmatch '^[0-9a-fA-F]{64}$' -or (Get-FileHash -LiteralPath $metadata.FullName).Hash -ine $ExpectedMetadataSha256){throw 'Release metadata pin mismatch.'}
    $pinMatched=$true
}
$record=Get-Content -LiteralPath $metadata.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
Assert-Keys $record @('schema','name','version','architecture','source_commit','signed','profile_session','profile_library','installer_compiler','assets')
if($record.schema -cne 1 -or $record.name -cne 'rust-browser' -or $record.architecture -cne 'x64' -or $record.source_commit -cnotmatch '^[0-9a-f]{40}$' -or $record.signed -isnot [bool] -or $record.signed -or $record.profile_session -cne 1 -or $record.profile_library -cne 3 -or $record.installer_compiler -cne 'Inno Setup 6.7.4'){throw 'Unsupported release metadata or signing claim.'}
$version=Parse-Version $record.version
if($version -lt $current -or ($version -eq $current -and -not $AllowSameVersion)){throw 'Refuse downgrade or unrequested same-version reinstall.'}
$expected=@("rust-browser-v$($record.version)-windows-x64.zip","rust-browser-v$($record.version)-windows-x64-setup.exe")
if(@($record.assets).Count -ne 2 -or ((@($record.assets.name | Sort-Object) -join '|') -cne (($expected | Sort-Object) -join '|'))){throw 'Unexpected, duplicate or unsafe release artifact names.'}
foreach($asset in $record.assets){
    Assert-Keys $asset @('name','bytes','sha256')
    if($asset.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $asset.bytes -is [string] -or $asset.bytes -le 0 -or $asset.bytes -gt 1073741824){throw 'Invalid artifact hash or length.'}
    $file=Assert-Ordinary (Join-Path $directory.FullName $asset.name)
    if($file.Length -ne $asset.bytes -or (Get-FileHash -LiteralPath $file.FullName).Hash.ToLowerInvariant() -cne $asset.sha256){throw 'Release artifact integrity mismatch.'}
}
# Never execute, extract, download, register, install or modify browser data here.
[pscustomobject]@{Version=$record.version;IntegrityVerified=$true;MetadataPinMatched=$pinMatched;PublisherAuthenticated=$false;Warning='Unsigned metadata and artifacts. A co-hosted checksum is not independent publisher authentication. Manual review/consent is still required.'}
