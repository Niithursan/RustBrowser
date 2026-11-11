param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][ValidatePattern('\A[0-9a-fA-F]{40}\z')][string]$SourceCommit,
    [Parameter(Mandatory=$true)][string]$Compiler
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
$base=Assert-PackageOutput $OutputRoot $repo
$production=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs/RustBrowser')).TrimEnd('\')
if($base -eq $production -or $base.StartsWith($production+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Do not prepare owner bundles inside the production installation.'}
$compilerPath=(Resolve-Path -LiteralPath $Compiler).Path
# Same pinned compiler/publisher checks as the production installer producer.
if((Get-FileHash -LiteralPath $compilerPath).Hash -ine 'f02fa0e5f237b47d7ed1edae9b991866ec94fe02af61eb6a73e94f92fe8e0604'){throw 'Use the pinned official Inno Setup 6.7.4 compiler.'}
$signature=Get-AuthenticodeSignature -LiteralPath $compilerPath
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike '*O=Pyrsys B.V.*'){throw 'Compiler publisher signature is not valid.'}
$version=([Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $Executable).Path)).ProductVersion
$package=& (Join-Path $repo 'packaging/windows/package.ps1') -Executable $Executable -OutputRoot $base -Version $version -SourceCommit $SourceCommit
$imports=& (Join-Path $PSScriptRoot 'check-crt.ps1') -Executable $Executable
$identity=[guid]::NewGuid().ToString('N')
$run=Join-Path $base ('o-'+$identity.Substring(0,8))
$app=Join-Path $run 'application'
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
Copy-Item -LiteralPath $package.payload -Destination (Join-Path $run 'portable') -Recurse
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'owner-launch.ps1') -Destination (Join-Path $run 'Launch.ps1')
Copy-Item -LiteralPath (Join-Path $repo 'packaging/windows/paths.ps1') -Destination (Join-Path $run 'paths.ps1')
& $compilerPath '/Q' ("/DAppVersion=$version") ("/DPayloadRoot=$($package.payload)") ("/DIconFile="+(Join-Path $repo 'packaging/windows/rust-browser.ico')) ("/DValidationId=$identity") ("/DValidationRoot=$app") ("/O$run") (Join-Path $repo 'packaging/windows/installer.iss')
if($LASTEXITCODE -ne 0){throw 'Validation-only installer compilation failed; do not use partial output.'}
$setup=Join-Path $run "rust-browser-v$version-windows-x64-setup.exe"
$record=[ordered]@{schema=1;version=$version;source_commit=$SourceCommit.ToLowerInvariant();sha256=$imports.sha256;identity=('RustBrowser.Validation.'+$identity);repository=$repo;installer_sha256=(Get-FileHash -LiteralPath $setup).Hash.ToLowerInvariant();crt_imports=$imports.crt_imports;qualified=$false;publication_authorized=$false}
[IO.File]::WriteAllText((Join-Path $run 'owner.json'),($record | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
$instructions=@'
# Disposable owner acceptance build

This unsigned validation build is not a release candidate or a production install.
Keep this directory outside Git and do not move it: Setup has a fixed destination
in its application subdirectory. Read owner.json for version, source commit,
executable SHA-256, validation identity, and Setup SHA-256. Verify supplied hashes
through the reviewed PR; local checksums alone do not authenticate the publisher.

From PowerShell opened in this directory:

    .\Launch.ps1 -VerifyOnly
    .\Launch.ps1

To inspect the installer wizard, run only the setup EXE in THIS directory. It uses
a distinct validation identity and the application subdirectory, never the normal
installation directory. It does not launch the browser. Inspect desktop/Start Menu
shortcut icons without opening them: shortcuts do not select a disposable profile.
After installation use:

    .\Launch.ps1 -Installed -VerifyOnly
    .\Launch.ps1 -Installed

Use the same command again for the checklist restart. Close the prior test window
first. Both launches explicitly use this directory's profile subdirectory and
verify the executable hash. The local new-tab page should appear on first launch.
Private windows inherit the test root. The real browser can stay open.

Follow validation/OWNER_CHECKLIST.md from the reviewed PR. Report pass, fail or
not tested for each item and the commit/hash from owner.json. Enter only your own
test credentials yourself. Do not share profile contents, credentials, private
room links or private screenshots. No account/device check is pre-approved as
passed. Do not enable Windows features or create a VM for these instructions.

The installer retains the production prerequisite checks, including the VC Runtime
check even for a static experiment. A missing-runtime host is not tested here.
No runtime was removed or replaced. To uninstall, select ONLY the Rust Browser
validation entry matching owner.json in Apps & Features. The separate test profile
is deliberately retained for investigation. This bundle never deletes a profile.
'@
[IO.File]::WriteAllText((Join-Path $run 'OWNER_BUILD.md'),$instructions,[Text.UTF8Encoding]::new($false))
[pscustomobject]@{root=$run;installer=$setup;launcher=(Join-Path $run 'Launch.ps1');manifest=(Join-Path $run 'owner.json');source_commit=$SourceCommit;sha256=$imports.sha256}
