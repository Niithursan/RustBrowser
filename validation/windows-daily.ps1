param([Parameter(Mandatory=$true)][string]$OutputRoot,[string]$Cargo = 'cargo',[string]$OpenSsl = '')
$ErrorActionPreference = 'Stop'
$base = [IO.Path]::GetFullPath($OutputRoot)
foreach($marker in @('session.json','session.lock','library.sqlite')){if(Test-Path -LiteralPath (Join-Path $base $marker)){throw 'Recognized browser profiles are not fixture output directories.'}}
$repo = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent)).TrimEnd('\')
if ($base -eq $repo -or $base.StartsWith($repo+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'Keep fixture keys outside the repository.'}
foreach ($protectedProfile in @($env:RUST_BROWSER_PROFILE, (Join-Path $env:LOCALAPPDATA 'RustBrowser'))) {
    if ($protectedProfile) { $path=[IO.Path]::GetFullPath($protectedProfile).TrimEnd('\'); if($base -eq $path -or $base.StartsWith($path+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Do not use a browser profile for fixtures.'} }
}
$ancestor=$base
while($ancestor){
    if(Test-Path -LiteralPath $ancestor){
        if((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Fixture output may not traverse reparse points.'}
        foreach($marker in @('session.json','session.lock','library.sqlite')){if(Test-Path -LiteralPath (Join-Path $ancestor $marker)){throw 'Recognized browser profiles are not fixture output directories.'}}
    }
    $parent=Split-Path $ancestor -Parent;if($parent -eq $ancestor){break};$ancestor=$parent
}
New-Item -ItemType Directory -Path $base -Force | Out-Null
$run = Join-Path $base ('daily-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
Set-Content -LiteralPath (Join-Path $run '.rust-browser-daily-validation') -Value 'Owned controlled fixtures'
$portFile = Join-Path $run 'tls-port.txt'
if (-not $OpenSsl) {
    $command=Get-Command openssl.exe -ErrorAction SilentlyContinue
    if($command){$OpenSsl=$command.Source}else{$git=(Get-Command git.exe -ErrorAction Stop).Source;$OpenSsl=Join-Path (Split-Path (Split-Path $git -Parent) -Parent) 'usr/bin/openssl.exe'}
}
if(-not(Test-Path -LiteralPath $OpenSsl -PathType Leaf)){throw 'Install Git for Windows with OpenSSL, or provide -OpenSsl.'}
$key=Join-Path $run 'tls-key.pem';$cert=Join-Path $run 'tls-cert.pem'
$generator=Start-Process -FilePath $OpenSsl -ArgumentList @('req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost,IP:127.0.0.1','-keyout',('"'+$key+'"'),'-out',('"'+$cert+'"')) -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $run 'certificate.stderr.txt') -RedirectStandardOutput (Join-Path $run 'certificate.stdout.txt')
$null=$generator.Handle
try{if(-not $generator.WaitForExit(30000)){Stop-Process -InputObject $generator;throw 'Owned certificate generation timed out.'};$generator.WaitForExit();if($generator.ExitCode -ne 0){throw 'Owned certificate generation failed.'}}finally{$generator.Dispose()}
$script = Join-Path $PSScriptRoot 'tls_fixture.py'
$arguments = '"' + $script + '" --root "' + $run + '"'
$server = Start-Process -FilePath (Get-Command python.exe -ErrorAction Stop).Source -ArgumentList $arguments -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $run 'tls.stdout.txt') -RedirectStandardError (Join-Path $run 'tls.stderr.txt')
$null = $server.Handle
$previous = $env:RUST_BROWSER_TLS_FIXTURE
try {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $port = ''
    while (-not $port) {
        if ($server.HasExited -or $clock.Elapsed.TotalSeconds -gt 15) { throw "TLS fixture could not start. Inspect $run." }
        if(Test-Path -LiteralPath $portFile){$port=([string](Get-Content -LiteralPath $portFile -Raw)).Trim()}
        if($port){break}
        Start-Sleep -Milliseconds 50
    }
    if ($port -notmatch '^\d+$' -or [int]$port -lt 1 -or [int]$port -gt 65535) {throw 'Invalid owned fixture port.'}
    $env:RUST_BROWSER_TLS_FIXTURE = "https://127.0.0.1:$port/"
    & $Cargo test --locked --bin rust-browser native_daily_navigation_diagnostics_and_modern_sites -- --ignored --test-threads=1 --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'Native daily-driver regression failed.' }
} finally {
    $env:RUST_BROWSER_TLS_FIXTURE = $previous
    if (-not $server.HasExited) { Stop-Process -InputObject $server }
    $server.WaitForExit(); $server.Dispose()
}
