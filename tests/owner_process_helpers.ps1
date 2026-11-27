# Process.Path reads MainModule, which can be empty before the loader initializes.
# Keep identity strict and share the caller's startup budget (20 seconds unless given).
function Assert-OwnerProcessImage([Diagnostics.Process]$Process,[string]$Expected,[Diagnostics.Stopwatch]$Clock,[int]$Seconds=20){
 do{
  $Process.Refresh()
  if($Process.HasExited){throw 'Owned browser exited before image verification'}
  $actual=$Process.Path
  if($actual){
   if($actual -ine $Expected){throw "Unexpected launched executable: actual='$actual', expected='$Expected'"}
   return
  }
  Start-Sleep -Milliseconds 25
 }while($Clock.Elapsed.TotalSeconds -lt $Seconds)
 throw 'Owned browser image was unavailable within the startup budget'
}
