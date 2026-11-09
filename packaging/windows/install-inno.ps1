param([Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..')).TrimEnd('\')
$base=[IO.Path]::GetFullPath($Destination).TrimEnd('\')
if($base -eq $repo -or $base.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Build tools must be outside the repository.'}
foreach($profile in @($env:RUST_BROWSER_PROFILE,(Join-Path $env:LOCALAPPDATA 'RustBrowser'))){if($profile){$profile=[IO.Path]::GetFullPath($profile).TrimEnd('\');if($base -eq $profile -or $base.StartsWith($profile+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Build tools must be outside browser profiles.'}}}
$ancestor=$base
while($ancestor){
    if(Test-Path -LiteralPath $ancestor){
        if((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Tool path may not traverse reparse points.'}
        foreach($marker in @('session.json','session.lock','library.sqlite')){if(Test-Path -LiteralPath (Join-Path $ancestor $marker)){throw 'Do not use a browser profile for build tools.'}}
    }
    $parent=Split-Path $ancestor -Parent;if($parent -eq $ancestor){break};$ancestor=$parent
}
New-Item -ItemType Directory -Path $base -Force | Out-Null
$download=Join-Path $base 'innosetup-6.7.4.exe'
if(-not(Test-Path -LiteralPath $download)){
    Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_4/innosetup-6.7.4.exe' -OutFile $download
}
if((Get-Item -LiteralPath $download -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Compiler download must be an ordinary file.'}
if((Get-FileHash -LiteralPath $download).Hash -ine 'a549929319f9120716677c685535ca4671e4b937b5fdc87e2585f32d8eb30e8c'){throw 'Compiler download hash mismatch.'}
$signature=Get-AuthenticodeSignature -LiteralPath $download
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike '*O=Pyrsys B.V.*'){throw 'Compiler publisher signature is not valid.'}
$directory=Join-Path $base 'inno'
$null=. (Join-Path $PSScriptRoot 'paths.ps1')
Assert-PlainPath $directory
$compiler=Join-Path $directory 'ISCC.exe'
if(-not(Test-Path -LiteralPath $compiler)){
    $process=Start-Process -FilePath $download -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER','/NOICONS',('/DIR="'+$directory+'"'),'/TASKS=') -PassThru -WindowStyle Hidden
    $null=$process.Handle
    try{if(-not $process.WaitForExit(60000)){throw 'Owned compiler setup timed out; inspect the process.'};$process.WaitForExit();if($process.ExitCode -ne 0){throw 'Compiler setup failed.'}}finally{$process.Dispose()}
}
if((Get-Item -LiteralPath $compiler -Force).Attributes -band [IO.FileAttributes]::ReparsePoint -or (Get-FileHash -LiteralPath $compiler).Hash -ine 'f02fa0e5f237b47d7ed1edae9b991866ec94fe02af61eb6a73e94f92fe8e0604'){throw 'Unexpected compiler payload.'}
$compilerSignature=Get-AuthenticodeSignature -LiteralPath $compiler
if($compilerSignature.Status -ne 'Valid' -or $compilerSignature.SignerCertificate.Subject -notlike '*O=Pyrsys B.V.*'){throw 'Compiler executable signature is not valid.'}
$compiler
