param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][ValidatePattern('^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$')][string]$Version,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-fA-F]{40}$')][string]$SourceCommit,
    [Parameter(Mandatory=$true)][string]$Compiler
)
$ErrorActionPreference='Stop'
foreach($part in $Version.Split('.')){if([long]$part -gt 65535){throw 'Version components must fit Windows metadata.'}}
$compilerPath=(Resolve-Path -LiteralPath $Compiler).Path
if((Get-FileHash -LiteralPath $compilerPath).Hash -ine 'f02fa0e5f237b47d7ed1edae9b991866ec94fe02af61eb6a73e94f92fe8e0604'){throw 'Use the pinned official Inno Setup 6.7.4 compiler.'}
$signature=Get-AuthenticodeSignature -LiteralPath $compilerPath
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike '*O=Pyrsys B.V.*'){throw 'Compiler publisher signature is not valid.'}
# The existing producer validates PE metadata, fixed contents and output ancestry.
$portable=& (Join-Path $PSScriptRoot 'package.ps1') -Executable $Executable -OutputRoot $OutputRoot -Version $Version -SourceCommit $SourceCommit
$run=Split-Path $portable.payload -Parent
$icon=Join-Path $PSScriptRoot 'rust-browser.ico'
& $compilerPath '/Q' ("/DAppVersion=$Version") ("/DPayloadRoot=$($portable.payload)") ("/DIconFile=$icon") ("/O$run") (Join-Path $PSScriptRoot 'installer.iss')
if($LASTEXITCODE -ne 0){throw 'Installer compilation failed.'}
$installer=Join-Path $run "rust-browser-v$Version-windows-x64-setup.exe"
$digest=(Get-FileHash -LiteralPath $installer).Hash.ToLowerInvariant()
$checksum=$installer+'.sha256'
[IO.File]::WriteAllText($checksum,($digest+'  '+(Split-Path $installer -Leaf)+"`n"),[Text.Encoding]::ASCII)
$assets=@($portable.archive,$installer) | ForEach-Object {
    $file=Get-Item -LiteralPath $_
    [ordered]@{name=$file.Name;bytes=$file.Length;sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}
}
$metadata=Join-Path $run "rust-browser-v$Version-windows-x64-release.json"
$record=[ordered]@{schema=1;name='rust-browser';version=$Version;architecture='x64';source_commit=$SourceCommit.ToLowerInvariant();signed=$false;profile_session=1;profile_library=3;installer_compiler='Inno Setup 6.7.4';assets=@($assets)}
[IO.File]::WriteAllText($metadata,(($record | ConvertTo-Json -Depth 5)+"`n"),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{installer=$installer;installer_checksum=$checksum;archive=$portable.archive;checksum=$portable.checksum;metadata=$metadata;payload=$portable.payload;version=$Version}
