param([Parameter(Mandatory=$true)][string]$TestRoot)
. (Join-Path $PSScriptRoot '../experiments/servo/tools.ps1')
function Require([bool]$Condition,[string]$Message){if(-not $Condition){throw $Message}}
function Reject([scriptblock]$Action,[string]$Message){
    $rejected=$false
    try{ & $Action | Out-Null }catch{ if($_.Exception.Message -like ('*'+$Message+'*')){$rejected=$true}else{throw} }
    Require $rejected ('Expected rejection: '+$Message)
}
$repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Reject {New-ServoRun (Join-Path $repository 'experimental-profile')} 'outside the repository'
$root=New-ServoRun $TestRoot
$first=New-ServoRun $root
$second=New-ServoRun $root
Require ($first -ne $second) 'Runs must never reuse a profile'
Require (Test-Path -LiteralPath (Join-Path $first '.rust-browser-servo-spike')) 'Missing ownership marker'
$profile=Join-Path $root 'existing-profile'
New-Item -ItemType Directory -Path $profile | Out-Null
Set-Content -LiteralPath (Join-Path $profile 'session.json') -Value 'original sentinel'
Reject {New-ServoRun (Join-Path $profile 'nested')} 'existing browser profile'
Require ((Get-Content -LiteralPath (Join-Path $profile 'session.json') -Raw).Trim() -eq 'original sentinel') 'Profile sentinel changed'
$junction=Join-Path $root 'redirect'
New-Item -ItemType Junction -Path $junction -Target $second | Out-Null
Reject {New-ServoRun (Join-Path $junction 'nested')} 'reparse point'
$fake=Join-Path $root 'invalid.exe'
Set-Content -LiteralPath $fake -Value 'not an executable'
Reject { & (Join-Path $PSScriptRoot '../experiments/servo/run.ps1') -ServoExecutable $fake -OutputRoot $root } 'pinned official Servo'
Reject { & (Join-Path $PSScriptRoot '../experiments/servo/prepare.ps1') -Archive $fake -OutputRoot $root } 'digest mismatch'
Reject { & (Join-Path $PSScriptRoot '../experiments/servo/isolation/check-boundaries.ps1') -OutputRoot $repository } 'outside the repository'
$arguments=@(Get-ServoArguments 'C:\disposable path\profile' 'https://example.com/')
Require ($arguments.Count -eq 4) 'Unexpected launch arguments'
Require ($arguments[0] -eq '--temporary-storage') 'Missing temporary storage restriction'
Require ($arguments[1] -eq '--window-size=1200x820') 'Unexpected initial window size'
Require ($arguments[2] -eq '--config-dir="C:\disposable path\profile"') 'Profile quoting failed'
Require ($arguments[3] -eq 'https://example.com/') 'Unexpected destination'
Require (-not ($arguments -match 'ignore-certificate|devtools|webdriver|userscripts|sandbox')) 'Unsafe launch arguments'
Get-ChildItem (Join-Path $PSScriptRoot '../experiments/servo') -Filter *.ps1 -Recurse | ForEach-Object {
    $tokens=$null;$errors=$null
    $null=[Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$tokens,[ref]$errors)
    Require ($errors.Count -eq 0) ('PowerShell syntax: '+$_.Name)
}
'Servo tooling: output isolation, unique runs, profile preservation, reparse rejection, digest refusal, quoting and syntax passed. No executable was launched.'
