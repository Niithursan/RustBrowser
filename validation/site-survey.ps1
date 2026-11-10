# Manual public-website survey through the production browser session.
# Not a CI gate: results depend on the network and on third-party websites.
# Uses a disposable profile, enters no credentials and grants no permissions.
param(
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [string[]]$Urls,
    [string]$UrlFile = '',
    [int]$TimeoutSeconds = 30,
    [int]$SettleMilliseconds = 2500,
    [switch]$KeepTabsOpen,
    [ValidateRange(0, 32)][int]$BurstCloseFillers = 0,
    [string]$Cargo = 'cargo'
)
$ErrorActionPreference = 'Stop'
$base = [IO.Path]::GetFullPath($OutputRoot)
$repo = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent)).TrimEnd('\')
if ($base -eq $repo -or $base.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Keep survey output outside the repository.' }
foreach ($marker in @('session.json', 'session.lock', 'library.sqlite')) {
    if (Test-Path -LiteralPath (Join-Path $base $marker)) { throw 'A browser profile is not a survey output directory.' }
}
if (-not $Urls) {
    if (-not $UrlFile) { $UrlFile = Join-Path $PSScriptRoot 'site-survey-sites.txt' }
    $Urls = @(Get-Content -LiteralPath $UrlFile | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') })
}
if ($Urls.Count -lt 1 -or $Urls.Count -gt 64) { throw 'Survey 1 to 64 addresses.' }
if ($BurstCloseFillers -gt 0 -and 1 + $BurstCloseFillers + $Urls.Count -gt 64) { throw 'A burst keeps every tab open: the start tab, fillers and addresses must fit in 64 tabs.' }
foreach ($url in $Urls) { if ($url -notmatch '^https?://[^\s@]+$') { throw "Not a plain HTTP(S) address: $url" } }
New-Item -ItemType Directory -Path $base -Force | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$plan = Join-Path $base "survey-$stamp-plan.json"
$output = Join-Path $base "survey-$stamp.json"
$json = @{ urls = @($Urls); output = $output; timeout_seconds = $TimeoutSeconds
    settle_milliseconds = $SettleMilliseconds; keep_tabs_open = [bool]$KeepTabsOpen
    burst_close_fillers = $BurstCloseFillers } | ConvertTo-Json
[IO.File]::WriteAllText($plan, $json, (New-Object Text.UTF8Encoding $false))
$previous = $env:RUST_BROWSER_SURVEY_PLAN
try {
    $env:RUST_BROWSER_SURVEY_PLAN = $plan
    & $Cargo test --locked --bin rust-browser native_public_site_survey -- --ignored --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "The survey did not complete. Partial results, if any: $output" }
} finally {
    $env:RUST_BROWSER_SURVEY_PLAN = $previous
}
$report = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
$report.visits | ForEach-Object {
    [pscustomobject]@{
        Address = $_.requested
        Result = if ($_.timed_out) { 'timed out' } elseif ($_.failed) { 'failed' } else { 'loaded' }
        Milliseconds = $_.elapsed_milliseconds
        Failures = $_.reported_failures.Count
        Downloads = $_.downloads_started
        Prompts = ($_.permission_prompts -join ',')
        Status = $_.message
    }
} | Format-Table -AutoSize -Wrap
"Engine exits: $($report.engine_exits); renderer exits: $($report.renderer_exits); crash reports: $($report.crash_reports)"
"Results: $output"
