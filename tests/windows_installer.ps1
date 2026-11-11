param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$BaselineExecutable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][string]$Compiler,
    [Parameter(Mandatory=$true)][string]$SourceCommit,
    [string]$Cargo='cargo'
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$version=([Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $Executable).Path)).ProductVersion
$package=& (Join-Path $repo 'packaging/windows/build-installer.ps1') -Executable $Executable -OutputRoot $OutputRoot -Version $version -SourceCommit $SourceCommit -Compiler $Compiler
$baselineVersion=([Diagnostics.FileVersionInfo]::GetVersionInfo((Resolve-Path -LiteralPath $BaselineExecutable).Path)).ProductVersion
if($baselineVersion -cne '0.17.0'){throw 'Supply the actual verified v0.17 executable for upgrade checks.'}
$baseline=& (Join-Path $repo 'packaging/windows/package.ps1') -Executable $BaselineExecutable -OutputRoot $OutputRoot -Version $baselineVersion -SourceCommit '7a61d974a23859933e5c252d192f345b68e6a532'
$run=Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ('install-validation-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
Set-Content -LiteralPath (Join-Path $run '.rust-browser-install-validation') -Value 'Owned disposable installer validation'
$identity=[guid]::NewGuid().ToString('N')
$app=Join-Path $run 'application'
$registry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RustBrowser.Validation.'+$identity+'_is1'
$link=Join-Path ([Environment]::GetFolderPath('Programs')) ("Rust Browser validation $identity/Rust Browser.lnk")
function Compile-Fixture($Payload,[string]$Version,[string]$Suffix,[string]$Script=(Join-Path $repo 'packaging/windows/installer.iss')) {
    $directory=Join-Path $run $Suffix;New-Item -ItemType Directory -Path $directory | Out-Null
    & $Compiler '/Q' ("/DAppVersion=$Version") ("/DPayloadRoot=$Payload") ("/DIconFile="+(Join-Path $repo 'packaging/windows/rust-browser.ico')) ("/DValidationId=$identity") ("/DValidationRoot=$app") ("/O$directory") $Script
    if($LASTEXITCODE -ne 0){throw 'Validation installer did not compile.'}
    Join-Path $directory "rust-browser-v$Version-windows-x64-setup.exe"
}
function Invoke-Owned([string]$Program,[string[]]$Arguments,[switch]$Refuse) {
    $process=Start-Process -FilePath $Program -ArgumentList $Arguments -PassThru -WindowStyle Hidden
    $null=$process.Handle
    try{
        if(-not $process.WaitForExit(60000)){throw 'Owned installer operation timed out; inspect its retained output.'}
        $process.WaitForExit();$code=$process.ExitCode
    }finally{$process.Dispose()}
    if(($Refuse -and $code -eq 0) -or (-not $Refuse -and $code -ne 0)){throw "Unexpected installer exit code: $code"}
}
function Args-For([string]$Step) { @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',('/LOG="'+(Join-Path $run "$Step.log")+'"')) }
function Hash-Profile {
    @('profile','downloads') | ForEach-Object {Get-ChildItem -LiteralPath (Join-Path $run $_) -File -Recurse} | Sort-Object FullName | ForEach-Object {($_.FullName.Substring($run.Length))+':'+(Get-FileHash -LiteralPath $_.FullName).Hash}
}
function Assert-Preserved {
    if(((Hash-Profile) -join '|') -cne ($script:before -join '|')){throw 'Installer operation changed disposable browsing/download data.'}
}
# The user's real default-browser choices and the production registration. Read
# only; every Setup run below must leave them exactly as they were.
function Real-Defaults {
    $choices = foreach ($kind in @('UrlAssociations\http', 'UrlAssociations\https', 'FileExts\.htm', 'FileExts\.html')) {
        $key = if ($kind -like 'Url*') { "HKCU:\Software\Microsoft\Windows\Shell\Associations\$kind\UserChoice" } else { "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\$kind\UserChoice" }
        $value = Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
        "${kind}=$($value.ProgId)/$($value.Hash)"
    }
    $registered = (Get-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -ErrorAction SilentlyContinue).'Rust Browser'
    @($choices) + @(
        "prog=$(Test-Path -LiteralPath 'HKCU:\Software\Classes\RustBrowserHTML')",
        "client=$(Test-Path -LiteralPath 'HKCU:\Software\Clients\StartMenuInternet\RustBrowser')",
        "registered=$registered")
}
$script:realDefaults = (Real-Defaults) -join '|'
$progId = 'RustBrowserV.' + $identity.Substring(0, 16)
$clientKey = "HKCU:\Software\Clients\StartMenuInternet\RustBrowser.Validation.$identity"
$registeredName = "Rust Browser validation $identity"
function Assert-Registration([bool]$Expected) {
    $prog = "HKCU:\Software\Classes\$progId"
    $value = (Get-ItemProperty -LiteralPath 'HKCU:\Software\RegisteredApplications' -ErrorAction SilentlyContinue).$registeredName
    if (-not $Expected) {
        if ((Test-Path -LiteralPath $prog) -or (Test-Path -LiteralPath $clientKey) -or $value) { throw 'Default-browser registration is present when it should not be.' }
        if (-not (Test-Path -LiteralPath 'HKCU:\Software\RegisteredApplications')) { throw 'The RegisteredApplications key itself was removed.' }
        return
    }
    $command = (Get-ItemProperty -LiteralPath "$prog\shell\open\command").'(default)'
    if ($command -cne ('"' + (Join-Path $app 'rust-browser.exe') + '" --single-argument %1')) { throw "Registered link command is wrong: $command" }
    $urls = Get-ItemProperty -LiteralPath "$clientKey\Capabilities\URLAssociations"
    if ($urls.http -cne $progId -or $urls.https -cne $progId) { throw 'http/https are not associated with the validation ProgID.' }
    if (@($urls.PSObject.Properties.Name | Where-Object { $_ -notlike 'PS*' }).Count -ne 2) { throw 'Only http and https may be registered.' }
    if ($value -cne "Software\Clients\StartMenuInternet\RustBrowser.Validation.$identity\Capabilities") { throw "RegisteredApplications value is wrong: $value" }
    $ini = Get-Content -LiteralPath (Join-Path $app 'installation.ini') -Raw
    if ($ini -notmatch [regex]::Escape("RegisteredName=$registeredName") -or $ini -notmatch 'Profile=') { throw 'installation.ini lacks the registered name or validation profile.' }
}
function Assert-Real-Defaults {
    if (((Real-Defaults) -join '|') -cne $script:realDefaults) { throw 'The real default browser, associations or production registration changed.' }
}
function Wait-Uninstalled {
    # The original uninstaller process exits before its cleanup child. Do not
    # race a new install against the still-owned uninstall executable/log.
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while(Test-Path -LiteralPath (Join-Path $app 'uninstall')){
        if($clock.Elapsed.TotalSeconds -gt 30){throw 'Owned uninstall cleanup did not remove its executable/log directory.'}
        Start-Sleep -Milliseconds 100
    }
    # The harness created the initially empty app directory for the unowned-file
    # refusal check. Inno correctly does not claim/delete that pre-existing root.
    if((Test-Path -LiteralPath (Join-Path $app 'installation.ini')) -or (Test-Path -LiteralPath (Join-Path $app 'rust-browser.exe'))){throw 'Owned uninstall payload remained.'}
}
$previousRoot=$env:RUST_BROWSER_INSTALL_FIXTURE;$previousStage=$env:RUST_BROWSER_INSTALL_STAGE
$installed=$false
try {
    $env:RUST_BROWSER_INSTALL_FIXTURE=$run;$env:RUST_BROWSER_INSTALL_STAGE='prepare'
    & $Cargo test --locked --test installation_profile installer_profile_harness -- --ignored --test-threads=1
    if($LASTEXITCODE -ne 0){throw 'Could not prepare representative profile.'}
    $script:before=@(Hash-Profile)
    # The baseline is built with the pinned v0.17 Setup script as well as its payload,
    # so the upgrade starts from what 0.17 really installed (including registration).
    $oldScript=Join-Path $run 'baseline-script';New-Item -ItemType Directory -Path $oldScript | Out-Null
    foreach($name in @('installer.iss','installer-notice.txt')){
        $text=git -C $repo show "7a61d974a23859933e5c252d192f345b68e6a532:packaging/windows/$name"
        if($LASTEXITCODE -ne 0){throw "Fetch the pinned v0.17 commit before building the upgrade fixture ($name)."}
        [IO.File]::WriteAllText((Join-Path $oldScript $name),(($text -join "`r`n")+"`r`n"))
    }
    $old=Compile-Fixture $baseline.payload $baselineVersion 'baseline-installer' (Join-Path $oldScript 'installer.iss')
    $new=Compile-Fixture $package.payload $version 'candidate-installer'
    New-Item -ItemType Directory -Path $app | Out-Null
    $foreign=Join-Path $app 'unowned.txt';[IO.File]::WriteAllText($foreign,'Synthetic file that must not be overwritten')
    $foreignHash=(Get-FileHash -LiteralPath $foreign).Hash
    Invoke-Owned $old (Args-For 'unowned-directory') -Refuse
    if((Get-FileHash -LiteralPath $foreign).Hash -cne $foreignHash){throw 'Unowned directory refusal changed data.'}
    Remove-Item -LiteralPath $foreign
    Invoke-Owned $old ((Args-For 'profile-directory')+('/DIR="'+(Join-Path $run 'profile')+'"')) -Refuse
    Assert-Preserved
    Invoke-Owned $old (Args-For 'fresh')
    $installed=$true;Assert-Preserved
    # 0.17 registered itself as a browser choice; the upgrade below must keep it.
    Assert-Registration $true;Assert-Real-Defaults
    if(-not(Test-Path -LiteralPath $link) -or -not(Test-Path -LiteralPath $registry)){throw 'Start Menu or per-user uninstall registration is missing.'}
    $entry=Get-ItemProperty -LiteralPath $registry
    if($entry.DisplayVersion -cne $baselineVersion -or $entry.InstallLocation.TrimEnd('\') -ine $app){throw 'Installed registration metadata is wrong.'}
    $desktop=Join-Path ([Environment]::GetFolderPath('Desktop')) "Rust Browser validation $identity.lnk"
    if(Test-Path -LiteralPath $desktop){throw 'Desktop shortcut was created without opt-in.'}
    $machineKey=$registry.Replace('HKCU:','HKLM:')
    if(Test-Path -LiteralPath $machineKey){throw 'Per-user installer wrote machine-wide registration.'}
    # A running old browser holds no marker for this validation identity (and v0.12
    # installs never had one). Its mapped executable must still block the upgrade.
    $previousProfile=$env:RUST_BROWSER_PROFILE
    $oldProcess=$null
    try {
        $env:RUST_BROWSER_PROFILE=Join-Path $run 'old-launch-profile'
        $oldProcess=Start-Process -FilePath (Join-Path $app 'rust-browser.exe') -PassThru -WindowStyle Hidden
        $null=$oldProcess.Handle
        $created=$oldProcess.StartTime.ToUniversalTime()
        Start-Sleep -Milliseconds 500
        if($oldProcess.HasExited){throw 'Owned v0.17 process exited before the file-use check.'}
        Invoke-Owned $new (Args-For 'legacy-running-refusal') -Refuse
        Assert-Preserved
    } finally {
        $env:RUST_BROWSER_PROFILE=$previousProfile
        if($oldProcess){
            . (Join-Path $repo 'benchmarks/process-tree.ps1')
            $children=@(Get-ProcessTreeIdentities (Get-CimInstance Win32_Process) $oldProcess.Id $created)
            if(-not $oldProcess.HasExited){$oldProcess.Kill();$oldProcess.WaitForExit()}
            $clock=[Diagnostics.Stopwatch]::StartNew()
            while(@(Live-ObservedChildren $children $oldProcess.Id).Count -gt 0){
                if($clock.Elapsed.TotalSeconds -gt 30){throw 'Observed owned old-browser children did not exit.'}
                Start-Sleep -Milliseconds 100
            }
            $oldProcess.Dispose()
        }
    }
    Invoke-Owned $new (Args-For 'upgrade');Assert-Preserved
    Assert-Registration $true;Assert-Real-Defaults
    $expectedFiles=@((Get-ChildItem -LiteralPath $package.payload -File).Name)+@('installation.ini')
    if(((@(Get-ChildItem -LiteralPath $app -File).Name | Sort-Object) -join '|') -cne (($expectedFiles | Sort-Object) -join '|')){throw 'Installed file allowlist mismatch.'}
    foreach($file in (Get-ChildItem -LiteralPath $package.payload -File)){
        if((Get-FileHash -LiteralPath (Join-Path $app $file.Name)).Hash -cne (Get-FileHash -LiteralPath $file.FullName).Hash){throw 'Installed content differs from the validated payload.'}
    }
    if((Get-ItemProperty -LiteralPath $registry).DisplayVersion -cne $version){throw 'Upgrade did not update Apps & Features version.'}
    if((Get-FileHash -LiteralPath (Join-Path $app 'rust-browser.exe')).Hash -cne (Get-FileHash -LiteralPath $Executable).Hash){throw 'Upgrade did not install the exact candidate bytes.'}
    $stdout=Join-Path $run 'version.stdout.txt'
    $process=Start-Process -FilePath (Join-Path $app 'rust-browser.exe') -ArgumentList '--version' -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout
    $null=$process.Handle
    try{if(-not $process.WaitForExit(30000)){throw 'Installed CLI launch timed out.'};$process.WaitForExit();if($process.ExitCode -ne 0 -or (Get-Content -LiteralPath $stdout -Raw).Trim() -cne "Rust Browser $version"){throw 'Installed version/loader launch failed.'}}finally{$process.Dispose()}
    Assert-Preserved
    if([version]$version -gt [version]$baselineVersion){Invoke-Owned $old (Args-For 'downgrade') -Refuse;Assert-Preserved}
    $marker=Join-Path $app 'installation.ini';$original=Get-Content -LiteralPath $marker -Raw
    [IO.File]::WriteAllText($marker,$original.Replace("Version=$version","Version=999.0.0"))
    Invoke-Owned $new (Args-For 'future-version') -Refuse
    [IO.File]::WriteAllText($marker,$original.Replace("Version=$version","Version=broken"))
    Invoke-Owned $new (Args-For 'invalid-version') -Refuse
    [IO.File]::WriteAllText($marker,$original)
    $mutex=[Threading.Mutex]::new($false,"Local\RustBrowser.Validation.$identity")
    try{
        Invoke-Owned $new (Args-For 'running-refusal') -Refuse
        Invoke-Owned (Join-Path $app 'uninstall/unins000.exe') (Args-For 'running-uninstall-refusal') -Refuse
    }finally{$mutex.Dispose()}
    $junction=Join-Path $app 'blocked-junction'
    New-Item -ItemType Junction -Path $junction -Target (Join-Path $run 'profile') | Out-Null
    try{Invoke-Owned $new (Args-For 'reparse-refusal') -Refuse}finally{
        if([IO.Path]::GetFullPath($junction) -cne (Join-Path $app 'blocked-junction') -or -not((Get-Item -LiteralPath $junction -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Refuse unexpected junction cleanup target.'}
        [IO.Directory]::Delete($junction,$false)
    }
    Assert-Preserved
    # A failed pre-install attempt must leave the prior usable executable intact.
    if((Get-FileHash -LiteralPath (Join-Path $app 'rust-browser.exe')).Hash -cne (Get-FileHash -LiteralPath $Executable).Hash){throw 'A refused install replaced application bytes.'}
    Invoke-Owned (Join-Path $app 'uninstall/unins000.exe') (Args-For 'uninstall')
    Wait-Uninstalled
    $installed=$false;Assert-Preserved
    if(Test-Path -LiteralPath (Join-Path $app 'rust-browser.exe')){throw 'Uninstall left the application executable.'}
    if((Test-Path -LiteralPath $link) -or (Test-Path -LiteralPath $registry)){throw 'Uninstall left shortcuts/registration.'}
    Assert-Registration $false;Assert-Real-Defaults
    Invoke-Owned $new ((Args-For 'reinstall')+'/TASKS=desktopicon');$installed=$true;Assert-Preserved
    Assert-Registration $true
    if(-not(Test-Path -LiteralPath $desktop)){throw 'Requested desktop shortcut is missing.'}
    $env:RUST_BROWSER_INSTALL_STAGE='verify'
    & $Cargo test --locked --test installation_profile installer_profile_harness -- --ignored --test-threads=1
    if($LASTEXITCODE -ne 0){throw 'Installed profile semantics changed.'}
    $verify=Join-Path $repo 'packaging/windows/verify-release.ps1'
    & $verify -MetadataPath $package.metadata -ArtifactDirectory (Split-Path $package.metadata) -CurrentVersion '0.0.0' | Out-Null
    $foreign=Join-Path $app 'user-added-fixture.txt';[IO.File]::WriteAllText($foreign,'Unowned synthetic data must survive uninstall')
    $foreignHash=(Get-FileHash -LiteralPath $foreign).Hash
    Invoke-Owned (Join-Path $app 'uninstall/unins000.exe') (Args-For 'unowned-file-uninstall')
    Wait-Uninstalled;$installed=$false;Assert-Preserved
    if(-not(Test-Path -LiteralPath $foreign) -or (Get-FileHash -LiteralPath $foreign).Hash -cne $foreignHash){throw 'Uninstall removed or changed an unowned file.'}
    if((Test-Path -LiteralPath $desktop) -or (Test-Path -LiteralPath $registry) -or (Test-Path -LiteralPath $link)){throw 'Uninstall left registered integration.'}
    Assert-Registration $false;Assert-Real-Defaults
    'Installer: fresh/0.17 upgrade/default-browser registration/full allowlist/CLI launch/downgrade/invalid version/running/reparse/profile refusal/uninstall/reinstall/optional shortcut and user-data preservation passed.'
} finally {
    try {
        if($installed -and (Test-Path -LiteralPath (Join-Path $app 'uninstall/unins000.exe'))){
            Invoke-Owned (Join-Path $app 'uninstall/unins000.exe') (Args-For 'final-uninstall')
            Wait-Uninstalled
        }
    } finally {
        $env:RUST_BROWSER_INSTALL_FIXTURE=$previousRoot;$env:RUST_BROWSER_INSTALL_STAGE=$previousStage
    }
}
Assert-Real-Defaults
$package
