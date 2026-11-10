param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$OutputRoot,[Parameter(Mandatory=$true)][string]$SourceCommit)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$version=([Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $Executable).Path)).ProductVersion
$packageScript=Join-Path $repo 'packaging/windows/package.ps1'
$one=& $packageScript -Executable $Executable -OutputRoot $OutputRoot -Version $version -SourceCommit $SourceCommit
$two=& $packageScript -Executable $Executable -OutputRoot $OutputRoot -Version $version -SourceCommit $SourceCommit
if($one.sha256 -cne $two.sha256){throw 'Identical package inputs produced different ZIP bytes.'}
if((Get-Content -LiteralPath $one.checksum -Raw).Trim() -cne ($one.sha256+'  '+(Split-Path $one.archive -Leaf))){throw 'Archive checksum file is incorrect.'}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=[IO.Compression.ZipFile]::OpenRead($one.archive)
try{
    $expected=@('rust-browser.exe','README.md','LICENSE','CHANGELOG.md','SECURITY.md','VALIDATION.md','INSTALL_WINDOWS.md','package.json') | Sort-Object
    $actual=@($zip.Entries.FullName | Sort-Object)
    if(($actual -join '|') -cne ($expected -join '|')){throw 'Package contains missing, duplicate or unexpected entries.'}
    foreach($entry in $zip.Entries){
        if($entry.FullName.Contains('/') -or $entry.FullName.Contains('\') -or $entry.LastWriteTime.Year -ne 2000 -or $entry.Length -gt 100MB){throw 'Invalid package entry or timestamp.'}
    }
    $reader=[IO.StreamReader]::new($zip.GetEntry('package.json').Open())
    try{$manifest=$reader.ReadToEnd() | ConvertFrom-Json}finally{$reader.Dispose()}
    if($manifest.schema -ne 1 -or $manifest.version -cne $version -or $manifest.architecture -cne 'x64' -or $manifest.source_commit -cne $SourceCommit.ToLowerInvariant() -or @($manifest.files).Count -ne 7){throw 'Package manifest metadata is incorrect.'}
    if($manifest.runtime -notlike '*WebView2*Visual C++ v14*neither bundled*'){throw 'Package must disclose both external runtime prerequisites.'}
    foreach($file in $manifest.files){
        if($file.name -notin $expected -or $file.sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Invalid file manifest.'}
        $entry=$zip.GetEntry($file.name)
        if($entry.Length -ne $file.bytes){throw 'Package length does not match the manifest.'}
        $stream=$entry.Open();$sha=[Security.Cryptography.SHA256]::Create()
        try{$hash=([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose();$stream.Dispose()}
        if($hash -cne $file.sha256){throw 'A packaged file does not match its recorded hash.'}
    }
}finally{$zip.Dispose()}
function Refused([scriptblock]$Action){$failed=$false;try{& $Action | Out-Null}catch{$failed=$true};if(-not $failed){throw 'Unsafe package input was accepted.'}}
. (Join-Path $repo 'packaging/windows/paths.ps1')
Refused { Assert-PackageOutput ([IO.Path]::GetPathRoot($repo)) $repo }
Refused { Assert-PackageOutput $repo $repo }
Refused { & $packageScript -Executable $Executable -OutputRoot $repo -Version $version -SourceCommit $SourceCommit }
Refused { & $packageScript -Executable $Executable -OutputRoot $OutputRoot -Version '999.0.0' -SourceCommit $SourceCommit }
Refused { & $packageScript -Executable $Executable -OutputRoot $OutputRoot -Version '../escape' -SourceCommit $SourceCommit }
$owned=Join-Path $OutputRoot ('safety-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $owned | Out-Null
$fake=Join-Path $owned 'rust-browser.exe';Set-Content -LiteralPath $fake -Value 'not a PE executable'
Refused { & $packageScript -Executable $fake -OutputRoot $OutputRoot -Version $version -SourceCommit $SourceCommit }
$protected=Join-Path $owned 'profile';New-Item -ItemType Directory -Path $protected | Out-Null
Set-Content -LiteralPath (Join-Path $protected 'library.sqlite') -Value 'synthetic data to preserve'
Refused { Assert-PackageOutput (Join-Path $protected 'nested') $repo }
$before=(Get-FileHash -LiteralPath (Join-Path $protected 'library.sqlite')).Hash
Refused { & $packageScript -Executable $Executable -OutputRoot $protected -Version $version -SourceCommit $SourceCommit }
Refused { & $packageScript -Executable $Executable -OutputRoot (Join-Path $protected 'nested/output') -Version $version -SourceCommit $SourceCommit }
Refused { & (Join-Path $repo 'validation/windows-daily.ps1') -OutputRoot $protected }
Refused { & (Join-Path $repo 'validation/windows-daily.ps1') -OutputRoot (Join-Path $protected 'nested/output') }
if($before -cne (Get-FileHash -LiteralPath (Join-Path $protected 'library.sqlite')).Hash -or @(Get-ChildItem -LiteralPath $protected).Count -ne 1){throw 'Refused packaging modified the synthetic profile.'}
$link=Join-Path $owned 'junction';$target=Join-Path $owned 'target';New-Item -ItemType Directory -Path $target | Out-Null
New-Item -ItemType Junction -Path $link -Target $target | Out-Null
Refused { Assert-PackageOutput (Join-Path $link 'output') $repo }
Refused { & $packageScript -Executable $Executable -OutputRoot (Join-Path $link 'output') -Version $version -SourceCommit $SourceCommit }
'Portable packaging: deterministic ZIP, checksums, x64/version metadata, manifest hashes, unsafe input/profile/reparse refusal passed.'
