param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$OutputRoot)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
. (Join-Path $repo 'packaging/windows/paths.ps1')
. (Join-Path $PSScriptRoot 'owner_process_helpers.ps1')
$base=Assert-PackageOutput $OutputRoot $repo
$exe=(Resolve-Path -LiteralPath $Executable).Path
if((Split-Path $exe -Leaf) -ine 'rust-browser.exe'){throw 'Use the owned browser test executable'}
$profile=Join-Path $base ('cold-'+[guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $profile | Out-Null
if(-not ('OwnerColdStartupFixture' -as [type])){
 # Test-only Win32 creation exposes the pre-loader state deterministically. It
 # changes no token/privilege/sandbox settings. Cleanup uses only returned handles.
 Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
public static class OwnerColdStartupFixture {
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct SI {
  public int cb; public string reserved,desktop,title;
  public int x,y,xSize,ySize,xCount,yCount,fill,flags; public short show,cbReserved;
  public IntPtr reserved2,stdin,stdout,stderr;
 }
 [StructLayout(LayoutKind.Sequential)] public struct PI {public IntPtr process,thread;public int pid,tid;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
 public static extern bool CreateProcessW(string app,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,int flags,IntPtr env,string cwd,ref SI si,out PI pi);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern uint ResumeThread(IntPtr thread);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
 [DllImport("kernel32.dll")] public static extern bool TerminateProcess(IntPtr process,uint code);
 public static Task ResumeAfter(IntPtr thread){return Task.Run(()=>{System.Threading.Thread.Sleep(500);if(ResumeThread(thread)==uint.MaxValue)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());});}
}
'@
}
$si=New-Object OwnerColdStartupFixture+SI;$si.cb=[Runtime.InteropServices.Marshal]::SizeOf($si)
$pi=New-Object OwnerColdStartupFixture+PI
$old=$env:RUST_BROWSER_PROFILE
try{
 $env:RUST_BROWSER_PROFILE=$profile
 if(-not [OwnerColdStartupFixture]::CreateProcessW($exe,[Text.StringBuilder]::new(('"'+$exe+'"')),[IntPtr]::Zero,[IntPtr]::Zero,$false,4,[IntPtr]::Zero,$base,[ref]$si,[ref]$pi)){throw [ComponentModel.Win32Exception]::new([Runtime.InteropServices.Marshal]::GetLastWin32Error())}
}finally{$env:RUST_BROWSER_PROFILE=$old}
$p=Get-Process -Id $pi.pid
$resume=$null
try{
 $missingBeforeResume=-not $p.Path
 $resume=[OwnerColdStartupFixture]::ResumeAfter($pi.thread)
 $clock=[Diagnostics.Stopwatch]::StartNew()
 Assert-OwnerProcessImage $p $exe $clock
 $wrongRejected=$false
 try{Assert-OwnerProcessImage $p ($exe+'.wrong') $clock}catch{$wrongRejected=$true}
 if(-not $wrongRejected){throw 'Image verification accepted a wrong nonempty path'}
 do{$p.Refresh();if($p.MainWindowHandle -ne 0 -and (Test-Path -LiteralPath (Join-Path $profile 'session.json'))){break};Start-Sleep -Milliseconds 100}while($clock.Elapsed.TotalSeconds -lt 20)
 if(-not $p.CloseMainWindow() -or -not $p.WaitForExit(20000)){throw 'Cold-start fixture did not close normally'}
 $exitRejected=$false
 try{Assert-OwnerProcessImage $p $exe $clock}catch{$exitRejected=$true}
 if(-not $exitRejected){throw 'Image verification accepted an exited process'}
 "Owner cold startup: pre-loader path missing=$missingBeforeResume; resolved identity, wrong-path/exit refusal and normal shutdown passed."
}finally{
 try{if($resume){$resume.Wait()}}finally{
  if(-not $p.HasExited){$null=[OwnerColdStartupFixture]::TerminateProcess($pi.process,1)}
  $null=[OwnerColdStartupFixture]::CloseHandle($pi.thread)
  $null=[OwnerColdStartupFixture]::CloseHandle($pi.process)
  $p.Dispose()
 }
}
