param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [Parameter(Mandatory=$true)][string]$Compiler,
    [Parameter(Mandatory=$true)][string]$SourceCommit,
    [Parameter(Mandatory=$true)][ValidateSet('static','dynamic')][string]$ExpectedCrt
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'owner_process_helpers.ps1')
$imports=& (Join-Path $repo 'validation/check-crt.ps1') -Executable $Executable -Expected $ExpectedCrt
$prepare=Join-Path $repo 'validation/prepare-owner-build.ps1'
$output=& $prepare -Executable $Executable -OutputRoot $OutputRoot -SourceCommit $SourceCommit -Compiler $Compiler
$bundle=$output | Where-Object {$_ -isnot [string] -and $_.manifest} | Select-Object -Last 1
if(-not $bundle){throw 'No owner bundle returned'}
$launcher=$bundle.launcher
$original=Get-Content -LiteralPath $bundle.manifest -Raw
$record=$original | ConvertFrom-Json
$prior=$env:RUST_BROWSER_PROFILE
function Refused([scriptblock]$Action){$failed=$false;try{& $Action | Out-Null}catch{$failed=$true};if(-not $failed){throw 'Unsafe owner input was accepted.'}}
$opposite=if($ExpectedCrt -eq 'static'){'dynamic'}else{'static'}
Refused { & (Join-Path $repo 'validation/check-crt.ps1') -Executable $Executable -Expected $opposite }
$dry=& $launcher -VerifyOnly
if($dry.will_launch -or $dry.profile -cne (Join-Path $bundle.root 'profile') -or $env:RUST_BROWSER_PROFILE -cne $prior -or (Test-Path -LiteralPath $dry.profile)){throw 'Verification launched, wrote a profile or changed the caller environment.'}
Refused { & $launcher -Installed -VerifyOnly }
$record.sha256='0'*64
[IO.File]::WriteAllText($bundle.manifest,($record | ConvertTo-Json))
Refused { & $launcher -VerifyOnly }
[IO.File]::WriteAllText($bundle.manifest,$original)
New-Item -ItemType Directory -Path $dry.profile | Out-Null
[IO.File]::WriteAllText((Join-Path $dry.profile 'session.json'),'synthetic foreign profile')
Refused { & $launcher -VerifyOnly }
Remove-Item -LiteralPath (Join-Path $dry.profile 'session.json')
# Exact empty owned test directory, non-recursive.
Remove-Item -LiteralPath $dry.profile
$localData=$env:LOCALAPPDATA
try{
 # Even a broken refusal must stay inside owned scratch, never a real profile.
 $env:LOCALAPPDATA=Join-Path $bundle.root 'synthetic-localappdata'
 foreach($protected in @((Join-Path $env:LOCALAPPDATA 'RustBrowser/v0.1'),(Join-Path $env:LOCALAPPDATA 'Programs/RustBrowser'))){
  Refused { & $prepare -Executable $Executable -OutputRoot $protected -SourceCommit $SourceCommit -Compiler $Compiler }
 }
}finally{$env:LOCALAPPDATA=$localData}
function Run-Owned([string]$Program,[string[]]$Arguments){
 $p=Start-Process -FilePath $Program -ArgumentList $Arguments -WindowStyle Hidden -PassThru
 $null=$p.Handle
 try{if(-not $p.WaitForExit(60000)){throw 'Owned installer operation timed out'};$p.WaitForExit();if($p.ExitCode -ne 0){throw "Owned installer failed: $($p.ExitCode)"}}finally{$p.Dispose()}
}
function Launch-And-Close([switch]$Installed){
 $launched=& $launcher -Installed:$Installed
 if($env:RUST_BROWSER_PROFILE -cne $prior){throw 'Owner launch changed the caller profile environment'}
 $p=Get-Process -Id $launched.process_id -ErrorAction Stop
 try{
  $expected=if($Installed){Join-Path $bundle.root 'application/rust-browser.exe'}else{Join-Path $bundle.root 'portable/rust-browser.exe'}
  $clock=[Diagnostics.Stopwatch]::StartNew()
  Assert-OwnerProcessImage $p $expected $clock
  do{$p.Refresh();if($p.HasExited){throw 'Owner browser exited before showing a window'};if($p.MainWindowHandle -ne 0 -and (Test-Path -LiteralPath (Join-Path $launched.profile 'session.json'))){break};Start-Sleep -Milliseconds 100}while($clock.Elapsed.TotalSeconds -lt 20)
  if($p.MainWindowHandle -eq 0 -or -not(Test-Path -LiteralPath (Join-Path $launched.profile 'session.json'))){throw 'No owner window and persisted disposable session'}
  if(-not $p.CloseMainWindow() -or -not $p.WaitForExit(20000)){throw 'Owner browser did not close normally'}
 }finally{
  if(-not $p.HasExited -and $p.Path -ieq $expected){Stop-Process -InputObject $p}
  $p.Dispose()
 }
}
Launch-And-Close
$installed=$false
try{
 Run-Owned $bundle.installer @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-')
 $installed=$true
 $ini=Join-Path $bundle.root 'application/installation.ini'
 $iniOriginal=[IO.File]::ReadAllText($ini)
 try{
  [IO.File]::WriteAllText($ini,$iniOriginal.Replace('Identity='+$record.identity,'Identity='+$record.identity+'x'))
  Refused { & $launcher -Installed -VerifyOnly }
 }finally{[IO.File]::WriteAllText($ini,$iniOriginal)}
 $dryInstalled=& $launcher -Installed -VerifyOnly
 if($dryInstalled.profile -cne $dry.profile){throw 'Installed launch selected a different profile'}
 Launch-And-Close -Installed
}finally{
 if($installed){
  $uninstaller=Join-Path $bundle.root 'application/uninstall/unins000.exe'
  # Identity and hash checks above completed before this exact validation uninstall.
  Run-Owned $uninstaller @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-')
  $clock=[Diagnostics.Stopwatch]::StartNew()
  while(Test-Path -LiteralPath (Join-Path $bundle.root 'application/uninstall')){
   if($clock.Elapsed.TotalSeconds -gt 30){throw 'Validation uninstall cleanup did not finish'}
   Start-Sleep -Milliseconds 100
  }
 }
}
if(-not(Test-Path -LiteralPath (Join-Path $dry.profile '.rust-browser-owner-profile'))){throw 'Owner profile marker missing'}
& (Join-Path $PSScriptRoot 'owner_process.ps1') -Executable (Join-Path $bundle.root 'portable/rust-browser.exe') -OutputRoot (Join-Path $bundle.root 'process-fixture')
"Owner bundle: hash/identity/foreign-profile/production-path refusal, dry-run, portable and installed GUI startup/shutdown, caller environment and disposable session preservation passed. Retained: $($bundle.root)"
