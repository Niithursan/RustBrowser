# Process.Path reads MainModule, which can be empty before the loader initializes.
# Keep identity strict and share the caller's existing 20-second startup budget.
function Assert-OwnerProcessImage([Diagnostics.Process]$Process,[string]$Expected,[Diagnostics.Stopwatch]$Clock){
 do{
  $Process.Refresh()
  if($Process.HasExited){throw 'Owned browser exited before image verification'}
  $actual=$Process.Path
  if($actual){
   if($actual -ine $Expected){throw "Unexpected launched executable: actual='$actual', expected='$Expected'"}
   return
  }
  Start-Sleep -Milliseconds 25
 }while($Clock.Elapsed.TotalSeconds -lt 20)
 throw 'Owned browser image was unavailable within the startup budget'
}
