# Manual UI Automation check of the browser toolbar, through the same Windows API
# that screen readers use. Not a screen-reader test and not a CI gate: it starts the
# real release build with a disposable profile and presses real Tab and Enter keys
# into its window, so it needs an unlocked interactive desktop. Before every key
# press it checks that the browser window is in the foreground and stops otherwise,
# so no key can reach another application.
#
# Checks:
#  1. Every visible interactive control in the toolbar document has a non-empty
#     accessible name and a control type a screen reader can announce.
#  2. Keyboard only: Tab from the address field reaches every visible enabled
#     toolbar control, including those behind the More (...) disclosure after it is
#     opened with Enter. Tabs use a roving tab stop: one tab is in the Tab order and
#     the others are reached with the arrow keys, which is checked separately.
param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputRoot,
    [int]$MaxPresses = 60
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class A11yInput {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc f, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
    // WebView2 exposes each document through its Chrome_RenderWidgetHostHWND child.
    public static System.Collections.Generic.List<IntPtr> RenderHosts(IntPtr parent) {
        var found = new System.Collections.Generic.List<IntPtr>();
        EnumChildWindows(parent, (h, l) => {
            var name = new System.Text.StringBuilder(64); GetClassName(h, name, 64);
            if (name.ToString() == "Chrome_RenderWidgetHostHWND") found.Add(h);
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static void Key(byte vk, bool shift) {
        if (shift) keybd_event(0x10, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, 2, UIntPtr.Zero);
        if (shift) keybd_event(0x10, 0, 2, UIntPtr.Zero);
    }
}
'@
$base = [IO.Path]::GetFullPath($OutputRoot)
New-Item -ItemType Directory -Force $base | Out-Null
$profile = Join-Path $base ('a11y-profile-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $profile | Out-Null
$previous = $env:RUST_BROWSER_PROFILE
$env:RUST_BROWSER_PROFILE = $profile
$process = Start-Process -FilePath $Executable -PassThru
$env:RUST_BROWSER_PROFILE = $previous
$UIA = [System.Windows.Automation.AutomationElement]
$Types = [System.Windows.Automation.ControlType]
$interactive = @($Types::Button, $Types::Edit, $Types::ComboBox, $Types::TabItem, $Types::CheckBox,
    $Types::RadioButton, $Types::MenuItem, $Types::Hyperlink, $Types::Slider, $Types::Spinner, $Types::ListItem)

function Foreground-Is-Browser {
    [uint32]$owner = 0; [void][A11yInput]::GetWindowThreadProcessId([A11yInput]::GetForegroundWindow(), [ref]$owner)
    return $owner -eq $process.Id
}
function Press([byte]$Key, [switch]$Shift) {
    if (-not (Foreground-Is-Browser)) { throw 'The browser window lost the foreground; stopped before pressing a key.' }
    [A11yInput]::Key($Key, [bool]$Shift)
    Start-Sleep -Milliseconds 150
}
function Toolbar-Document([IntPtr]$Window) {
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    foreach ($h in [A11yInput]::RenderHosts($Window)) {
        # The host pane's child is the document. A descendant search from the host
        # returns the document's contents but not the document itself.
        $hostElement = $UIA::FromHandle($h)
        # A full query makes Chromium build the document's accessibility tree.
        [void]$hostElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $candidate = $walker.GetFirstChild($hostElement)
        while ($candidate) {
            $c = $candidate.Current
            if ($c.ControlType -eq $Types::Document -and $c.Name -eq 'Browser controls') { return $candidate }
            $candidate = $walker.GetNextSibling($candidate)
        }
    }
}
function Controls($Document) {
    $all = $Document.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($e in $all) {
        $c = $e.Current
        if ($interactive -contains $c.ControlType -and -not $c.IsOffscreen) {
            [pscustomobject]@{ id = $c.AutomationId; type = $c.ControlType.ProgrammaticName -replace 'ControlType\.', ''
                name = $c.Name; enabled = $c.IsEnabled; focusable = $c.IsKeyboardFocusable }
        }
    }
}
function Key-Of($Element) {
    if (-not $Element) { return '' }
    $c = $Element.Current
    "$($c.ControlType.ProgrammaticName -replace 'ControlType\.', ''):$($c.AutomationId):$($c.Name)"
}
function In-Toolbar($Element, $Document) {
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $node = $Element
    while ($node) {
        if ([System.Windows.Automation.Automation]::Compare($node, $Document)) { return $true }
        $node = $walker.GetParent($node)
    }
    return $false
}
function Tab-Walk($Document, [string]$Label) {
    $seen = [ordered]@{}
    for ($i = 0; $i -lt $MaxPresses; $i++) {
        Press 0x09
        $focused = $UIA::FocusedElement
        if (-not (In-Toolbar $focused $Document)) { break }
        $key = Key-Of $focused
        if ($seen.Contains($key)) { break }
        $seen[$key] = $i + 1
    }
    "${Label}: $($seen.Count) toolbar stops before leaving or wrapping"
    $seen
}

$report = [ordered]@{ executable = $Executable; problems = @() }
try {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($process.MainWindowHandle -eq 0) {
        if ($clock.Elapsed.TotalSeconds -gt 30) { throw 'The browser window did not appear.' }
        Start-Sleep -Milliseconds 200; $process.Refresh()
    }
    $hwnd = $process.MainWindowHandle
    $document = $null
    while (-not $document) {
        if ($clock.Elapsed.TotalSeconds -gt 45) { throw 'The toolbar document did not appear in UI Automation.' }
        Start-Sleep -Milliseconds 500
        # The process can own more than one top-level window; read it again each time.
        $process.Refresh()
        if ($process.MainWindowHandle -ne 0) { $hwnd = $process.MainWindowHandle }
        $document = Toolbar-Document $hwnd
    }
    Start-Sleep -Seconds 2
    [void][A11yInput]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 500
    # Windows lets a process take the foreground only in some cases (for example
    # when started from the window in use). Without it no key is pressed; the name
    # and role checks still run. Keyboard reachability is also covered without OS
    # input by native_toolbar_keyboard_reachability_and_scaled_layout.
    $keyboard = Foreground-Is-Browser
    $report.keyboard_walk = if ($keyboard) { 'run' } else { 'not run: Windows did not give the browser window the foreground; no key was pressed' }

    # 1. Names and roles of everything visible.
    $controls = @(Controls $document)
    $report.controls = $controls
    foreach ($c in $controls) {
        if ([string]::IsNullOrWhiteSpace($c.name)) { $report.problems += "No accessible name: $($c.type) #$($c.id)" }
        if ($c.enabled -and -not $c.focusable -and $c.type -ne 'ListItem') { $report.problems += "Not keyboard focusable: $($c.type) '$($c.name)'" }
    }

    # 2. Tab through the toolbar from the address field.
    if ($keyboard) {
    $address = $document.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition($UIA::AutomationIdProperty, 'address')))
    if (-not $address) { throw 'The address field is not exposed to UI Automation.' }
    $address.SetFocus(); Start-Sleep -Milliseconds 300
    $start = Key-Of $UIA::FocusedElement
    $pass = Tab-Walk $document 'Closed More'
    $pass[0]
    $reached = @($start) + @($pass[1].Keys)
    # Wrap around once more from the start so controls before the address field count.
    $address.SetFocus(); Start-Sleep -Milliseconds 300
    for ($i = 0; $i -lt $MaxPresses; $i++) {
        Press 0x09 -Shift
        $focused = $UIA::FocusedElement
        if (-not (In-Toolbar $focused $document)) { break }
        $key = Key-Of $focused
        if ($reached -contains $key -and $key -ne $start) { break }
        $reached += $key
    }
    # Open More with the keyboard and walk again.
    $more = $document.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition($UIA::AutomationIdProperty, 'more')))
    $more.SetFocus(); Start-Sleep -Milliseconds 300
    Press 0x0D
    # The toolbar re-renders after the disclosure opens; wait for its contents.
    $expanded = $false
    $wait = [Diagnostics.Stopwatch]::StartNew()
    while (-not $expanded -and $wait.Elapsed.TotalSeconds -lt 3) {
        Start-Sleep -Milliseconds 200
        # @() because Windows PowerShell 5.1 gives a single object no Count.
        $expanded = @(@(Controls $document) | Where-Object { $_.id -eq 'pin-active' }).Count -gt 0
    }
    if (-not $expanded) { $report.problems += 'Enter on More did not show the tab and library actions.' }
    $opened = @(Controls $document)
    $report.controls_with_more_open = $opened
    $pass2 = Tab-Walk $document 'Open More'
    $pass2[0]
    $reached += @($pass2[1].Keys)

    $tabs = @($opened | Where-Object { $_.type -eq 'TabItem' })
    # Each tab's close button is kept out of the Tab order on purpose (Ctrl+W and
    # Ctrl+F4 close tabs from the keyboard).
    foreach ($c in ($opened | Where-Object { $_.enabled -and $_.type -ne 'TabItem' -and $_.type -ne 'ListItem' -and -not $_.name.StartsWith('Close tab ') })) {
        $key = "$($c.type):$($c.id):$($c.name)"
        if ($reached -notcontains $key) { $report.problems += "Not reached with Tab: $($c.type) '$($c.name)' #$($c.id)" }
    }
    if ($tabs.Count -gt 0 -and -not @($reached | Where-Object { $_ -like 'TabItem:*' }).Count) { $report.problems += 'No browser tab is in the Tab order.' }
    $report.reached = $reached
    }
    "Keyboard walk: $($report.keyboard_walk)"
    "Visible interactive controls: $($controls.Count)$(if ($keyboard) { "; $($opened.Count) with More open; reached by keyboard: $(@($reached | Select-Object -Unique).Count)" })"
} finally {
    if (-not $process.HasExited) { [void]$process.CloseMainWindow(); if (-not $process.WaitForExit(15000)) { $process.Kill() } }
    Start-Sleep -Milliseconds 500
    Remove-Item -LiteralPath $profile -Recurse -Force -ErrorAction SilentlyContinue
}
$json = Join-Path $base 'accessibility-uia.json'
[IO.File]::WriteAllText($json, ($report | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
$(if ($report.controls_with_more_open) { $report.controls_with_more_open } else { $report.controls }) | Format-Table type, name, id, enabled, focusable -AutoSize
"Results: $json"
if ($report.problems) { throw ("Accessibility problems:`n" + ($report.problems -join "`n")) }
if ($keyboard) { 'UI Automation: every visible toolbar control has a name and role, and every enabled one was reached with the keyboard.' }
else { 'UI Automation: every visible toolbar control has a name and role. The keyboard walk did not run (no foreground).' }
