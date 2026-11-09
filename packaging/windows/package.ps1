param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-fA-F]{40}$')][string]$SourceCommit
)
$ErrorActionPreference = 'Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..')).TrimEnd('\')
$base=[IO.Path]::GetFullPath($OutputRoot)
foreach($marker in @('session.json','session.lock','library.sqlite')){if(Test-Path -LiteralPath (Join-Path $base $marker)){throw 'Recognized browser profiles are not package output directories.'}}
if($base -eq $repo -or $base.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Choose packaging output outside the repository.'}
foreach($protectedProfile in @($env:RUST_BROWSER_PROFILE,(Join-Path $env:LOCALAPPDATA 'RustBrowser'))){
    if($protectedProfile){$path=[IO.Path]::GetFullPath($protectedProfile).TrimEnd('\');if($base -eq $path -or $base.StartsWith($path+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Do not write packages inside a browser profile.'}}
}
$ancestor=$base
while($ancestor){
    if(Test-Path -LiteralPath $ancestor){
        if((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Package output may not traverse reparse points.'}
        foreach($marker in @('session.json','session.lock','library.sqlite')){if(Test-Path -LiteralPath (Join-Path $ancestor $marker)){throw 'Recognized browser profiles are not package output directories.'}}
    }
    $parent=Split-Path $ancestor -Parent;if($parent -eq $ancestor){break};$ancestor=$parent
}
$exe=(Resolve-Path -LiteralPath $Executable).Path
if((Split-Path $exe -Leaf) -ine 'rust-browser.exe' -or (Get-Item -LiteralPath $exe).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Supply the built, ordinary rust-browser.exe.'}
$reader=[IO.BinaryReader]::new([IO.File]::OpenRead($exe))
try {
    if($reader.BaseStream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d){throw 'Not a PE executable.'}
    $reader.BaseStream.Position=0x3c;$offset=$reader.ReadUInt32()
    if($offset -lt 64 -or $offset -gt $reader.BaseStream.Length-6){throw 'Invalid PE header.'}
    $reader.BaseStream.Position=$offset
    if($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664){throw 'Package requires a Windows x64 executable.'}
} finally {$reader.Dispose()}
$info=[Diagnostics.FileVersionInfo]::GetVersionInfo($exe)
if($info.ProductName -cne 'Rust Browser' -or $info.ProductVersion -cne $Version -or $info.FileVersion -cne $Version){throw 'Executable metadata must match the requested application version.'}
$signature=(Get-AuthenticodeSignature -LiteralPath $exe).Status.ToString()
New-Item -ItemType Directory -Path $base -Force | Out-Null
$run=Join-Path $base ('package-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
Set-Content -LiteralPath (Join-Path $run '.rust-browser-package') -Value 'Owned portable package output'
$payload=Join-Path $run 'payload';New-Item -ItemType Directory -Path $payload | Out-Null
$sources=@($exe)+@('README.md','LICENSE','CHANGELOG.md','SECURITY.md','VALIDATION.md','INSTALL_WINDOWS.md' | ForEach-Object {Join-Path $repo $_})
$files=@()
foreach($sourceFile in $sources){
    $item=Get-Item -LiteralPath $sourceFile -ErrorAction Stop
    if($item.PSIsContainer -or $item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Only ordinary, allowlisted package files are accepted.'}
    $destination=Join-Path $payload $item.Name
    Copy-Item -LiteralPath $sourceFile -Destination $destination -ErrorAction Stop
    $files+= [ordered]@{name=$item.Name;bytes=$item.Length;sha256=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$manifest=[ordered]@{schema=1;name='rust-browser';version=$Version;architecture='x64';source_commit=$SourceCommit.ToLowerInvariant();authenticode_status=$signature;runtime='Microsoft Evergreen WebView2 Runtime and x64 Microsoft Visual C++ v14 Runtime required; neither bundled';files=@($files | Sort-Object name)}
$encoding=[Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText((Join-Path $payload 'package.json'),(($manifest | ConvertTo-Json -Depth 6)+"`n"),$encoding)
Add-Type -AssemblyName System.IO.Compression
$archive=Join-Path $run "rust-browser-v$Version-windows-x64.zip"
$stream=[IO.File]::Open($archive,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
try{
    $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$true)
    try{
        foreach($item in (Get-ChildItem -LiteralPath $payload -File | Sort-Object Name)){
            $entry=$zip.CreateEntry($item.Name,[IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime=[DateTimeOffset]::new(2000,1,1,0,0,0,[TimeSpan]::Zero)
            $inputStream=[IO.File]::OpenRead($item.FullName);$outputStream=$entry.Open()
            try{$inputStream.CopyTo($outputStream)}finally{$outputStream.Dispose();$inputStream.Dispose()}
        }
    }finally{$zip.Dispose()}
}finally{$stream.Dispose()}
$digest=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
$checksum=$archive+'.sha256'
[IO.File]::WriteAllText($checksum,($digest+'  '+(Split-Path $archive -Leaf)+"`n"),[Text.Encoding]::ASCII)
[pscustomobject]@{archive=$archive;checksum=$checksum;sha256=$digest;payload=$payload;version=$Version;authenticode_status=$signature}
