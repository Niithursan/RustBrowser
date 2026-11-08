Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'

function Get-ServoPin {
    Get-Content -LiteralPath (Join-Path $PSScriptRoot 'pin.json') -Raw | ConvertFrom-Json
}

function Assert-OutsideBrowserData([string]$Path) {
    $candidate=[IO.Path]::GetFullPath($Path).TrimEnd('\','/')
    $repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..')).TrimEnd('\','/')
    $forbidden=@($repository)
    if($env:LOCALAPPDATA){$forbidden+=Join-Path $env:LOCALAPPDATA 'RustBrowser'}
    if($env:RUST_BROWSER_PROFILE){$forbidden+=[IO.Path]::GetFullPath($env:RUST_BROWSER_PROFILE)}
    foreach($root in $forbidden){
        $root=$root.TrimEnd('\','/')
        if($candidate.Equals($root,[StringComparison]::OrdinalIgnoreCase) -or $candidate.StartsWith($root+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){
            throw 'Experimental output must be outside the repository and normal browser profiles.'
        }
    }
    # Recognize other existing profiles rather than writing experimental data into them.
    $ancestor=$candidate
    while($ancestor){
        if((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){
            throw 'Experimental output cannot traverse a reparse point.'
        }
        foreach($name in @('session.json','session.lock','library.sqlite')){
            if(Test-Path -LiteralPath (Join-Path $ancestor $name)){throw 'Experimental output cannot be inside an existing browser profile.'}
        }
        $parent=[IO.Directory]::GetParent($ancestor)
        $ancestor=if($parent){$parent.FullName}else{$null}
    }
    return $candidate
}

function New-ServoRun([string]$OutputRoot) {
    $root=Assert-OutsideBrowserData $OutputRoot
    # Never reuse a caller's profile or overwrite an existing run, even with a marker.
    $run=Join-Path $root ('servo-spike-'+[guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $run | Out-Null
    Set-Content -LiteralPath (Join-Path $run '.rust-browser-servo-spike') -Value 'Disposable experimental data; not a browser profile to reuse.' -Encoding utf8
    return $run
}

function Assert-ServoExecutable([string]$Executable) {
    $pin=Get-ServoPin
    $resolved=(Resolve-Path -LiteralPath $Executable).Path
    if((Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pin.executable_sha256){throw 'Executable does not match the pinned official Servo build.'}
    return $resolved
}

function Get-ServoArguments([string]$Profile,[string]$Url) {
    if($Profile.Contains('"') -or $Profile.Contains("`r") -or $Profile.Contains("`n")){throw 'Invalid experimental profile path.'}
    # No arbitrary arguments, certificate overrides, userscripts, debugging servers or bridge.
    @('--temporary-storage','--window-size=1200x820',('--config-dir="'+$Profile+'"'),$Url)
}
