# Shared, non-destructive output/path preflight. Never resolve through a junction.
function Assert-PlainPath([string]$Path) {
    $ancestor=[IO.Path]::GetFullPath($Path)
    while($ancestor){
        if(Test-Path -LiteralPath $ancestor){
            if((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Path may not traverse reparse points.'}
        }
        $parent=Split-Path $ancestor -Parent;if($parent -eq $ancestor){break};$ancestor=$parent
    }
}
function Assert-PackageOutput([string]$Path,[string]$Repository) {
    $absolute=[IO.Path]::GetFullPath($Path)
    if($absolute -cnotmatch '\A[A-Za-z]:\\' -or $absolute -eq [IO.Path]::GetPathRoot($absolute)){throw 'Use a named local output directory, not a drive/share root.'}
    $base=$absolute.TrimEnd('\')
    $repo=[IO.Path]::GetFullPath($Repository).TrimEnd('\')
    if($base -eq $repo -or $base.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Output must be outside the repository.'}
    foreach($protected in @($env:RUST_BROWSER_PROFILE,(Join-Path $env:LOCALAPPDATA 'RustBrowser'))){
        if($protected){$profile=[IO.Path]::GetFullPath($protected).TrimEnd('\');if($base -eq $profile -or $base.StartsWith($profile+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Output must be outside browser profiles.'}}
    }
    Assert-PlainPath $base
    $ancestor=$base
    while($ancestor){
        foreach($marker in @('session.json','session.lock','library.sqlite')){if(Test-Path -LiteralPath (Join-Path $ancestor $marker)){throw 'Recognized profiles are not output directories.'}}
        $parent=Split-Path $ancestor -Parent;if($parent -eq $ancestor){break};$ancestor=$parent
    }
    $base
}
