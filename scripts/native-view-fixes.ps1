param(
    [string]$Executable = 'D:\CodeBush\bin\codebush.exe',
    [string]$Source = '\\wsl.localhost\Ubuntu\home\mdwbr\codebush',
    [string]$OpenSource = '\\wsl.localhost\Ubuntu\dev\shm\codetree-benchmarks\ripgrep',
    [string]$Query = 'camera.rs',
    [string]$Output = 'D:\CodeBush\evidence\12d-native-4',
    [switch]$TreeStart
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CodeBushWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int x, y; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint first, uint second, bool attach);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@
[CodeBushWindow]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $Output | Out-Null
$report = Join-Path (Split-Path (Split-Path $Executable -Parent) -Parent) 'evidence\12d-latest\load-report.json'
$started = Get-Date
$process = Start-Process $Executable -ArgumentList ('"' + $Source + '" ' + $(if ($TreeStart) {'--tree '} else {''}) + '--native-timing "' + $Output + '\native-timing.json"') -WorkingDirectory 'D:\CodeBush' -PassThru -RedirectStandardOutput "$Output\stdout.log" -RedirectStandardError "$Output\stderr.log"
$shell = New-Object -ComObject WScript.Shell
function Wait-Report([datetime]$After) {
    $deadline = (Get-Date).AddSeconds(180)
    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited) { throw "Viewer exited: $($process.ExitCode)" }
        if ((Test-Path $report) -and (Get-Item $report).LastWriteTime -gt $After) {
            $loaded = Get-Content $report -Raw | ConvertFrom-Json
            $leaf = $loaded.root.TrimEnd('\').Split('\')[-1]
            $process.Refresh()
            if ($process.MainWindowTitle.EndsWith($leaf)) {
                Start-Sleep -Seconds 3
                return $loaded
            }
        }
        if ($script:LoadingShot -and [CodeBushWindow]::GetForegroundWindow() -eq $process.MainWindowHandle) {
            $state = Snapshot 'loading-probe'
            if ($state.loading) {
                Shot $script:LoadingShot
                $script:LoadingShot = $null
            }
        }
        Start-Sleep -Milliseconds 250
    }
    throw 'Viewer load timed out'
}
function Keys([string]$Value) {
    $foreground = [CodeBushWindow]::GetForegroundWindow()
    $owner = [uint32]0
    [CodeBushWindow]::GetWindowThreadProcessId($foreground, [ref]$owner) | Out-Null
    if ($owner -ne $process.Id) { throw 'Refusing to send keys to another application' }
    [System.Windows.Forms.SendKeys]::SendWait($Value)
    Start-Sleep -Milliseconds 180
}
function Shot([string]$Name) {
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ([CodeBushWindow]::GetForegroundWindow() -ne $window) { throw "Viewer must be foreground for its screenshot; viewer $window foreground $([CodeBushWindow]::GetForegroundWindow())" }
    $rect = New-Object CodeBushWindow+Rect
    $point = New-Object CodeBushWindow+Point
    [CodeBushWindow]::GetClientRect($window, [ref]$rect) | Out-Null
    [CodeBushWindow]::ClientToScreen($window, [ref]$point) | Out-Null
    $bitmap = New-Object System.Drawing.Bitmap ($rect.right - $rect.left), ($rect.bottom - $rect.top)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($point.x, $point.y, 0, 0, $bitmap.Size)
    $bitmap.Save("$Output\$Name.png", [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
}

function Click([double]$X, [double]$Y) {
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ([CodeBushWindow]::GetForegroundWindow() -ne $window) { throw 'Refusing click outside viewer' }
    $point = New-Object CodeBushWindow+Point
    [CodeBushWindow]::ClientToScreen($window, [ref]$point) | Out-Null
    $dpi = [CodeBushWindow]::GetDpiForWindow($window) / 96.0
    [CodeBushWindow]::SetCursorPos([int]($point.x + $X * $dpi), [int]($point.y + $Y * $dpi)) | Out-Null
    Start-Sleep -Milliseconds 100
    [CodeBushWindow]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [CodeBushWindow]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 200
}
function Snapshot([string]$Name) {
    $before = Get-Date
    Keys '{F12}'
    $statePath = "$Output\native-timing.state.json"
    if (-not (Test-Path $statePath) -or (Get-Item $statePath).LastWriteTime -lt $before) { throw 'Interaction snapshot is not fresh' }
    Copy-Item $statePath "$Output\$Name-state.json"
    return Get-Content $statePath -Raw | ConvertFrom-Json
}
function Same-Camera($First, $Second) {
    $zoomError = [Math]::Abs($First.camera_zoom - $Second.camera_zoom) / [Math]::Max($First.camera_zoom, 0.000001)
    $screenError = [Math]::Max([Math]::Abs($First.camera_center[0] - $Second.camera_center[0]), [Math]::Abs($First.camera_center[1] - $Second.camera_center[1])) * $First.camera_zoom
    return $zoomError -lt 0.001 -and $screenError -lt 0.5
}
try {
    $initial = Wait-Report $started
    Copy-Item $report "$Output\initial-load.json"
    $process.Refresh()
    [CodeBushWindow]::ShowWindow($process.MainWindowHandle, 9) | Out-Null
    $foregroundOwner = [uint32]0
    $foregroundThread = [CodeBushWindow]::GetWindowThreadProcessId([CodeBushWindow]::GetForegroundWindow(), [ref]$foregroundOwner)
    $currentThread = [CodeBushWindow]::GetCurrentThreadId()
    [CodeBushWindow]::AttachThreadInput($currentThread, $foregroundThread, $true) | Out-Null
    $shell.AppActivate($process.Id) | Out-Null
    [CodeBushWindow]::SetForegroundWindow($process.MainWindowHandle) | Out-Null
    [CodeBushWindow]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 100, 100, 2200, 1500, 0) | Out-Null
    [CodeBushWindow]::AttachThreadInput($currentThread, $foregroundThread, $false) | Out-Null
    Start-Sleep -Seconds 1
    $first = Snapshot 'initial'
    if ($TreeStart) {
        if ($first.mode -ne 'tree') { throw 'CLI tree startup failed' }
        Shot '00-tree-start'
        Click 225 35
    }
    $space = Snapshot 'space'
    if ($space.mode -ne '12d') { throw 'Expected 12D mode' }
    $loadStamp = (Get-Item $report).LastWriteTime
    Click 225 35
    $tree = Snapshot 'tree'
    if ($tree.mode -ne 'tree' -or $tree.resident_bytes -ne $space.resident_bytes) { throw 'GUI tree switch changed residency or mode' }
    Shot '01-tree-overview'
    Keys '^k'
    Keys $Query
    Start-Sleep -Seconds 2
    $found = Snapshot 'tree-search'
    if ($found.search_matches -lt 1) { throw 'Tree source search failed' }
    Shot '02-tree-search'
    Keys '{ENTER}'
    Start-Sleep -Seconds 1
    $firstMatch = Snapshot 'tree-first-match'
    if ($firstMatch.active_match -ne 0 -or $firstMatch.selected_path -ne $firstMatch.match_path -or $firstMatch.match_line -ne 0 -or -not $firstMatch.selected_path.EndsWith($Query)) { throw 'First Enter did not open the exact filename match' }
    Shot '03-tree-read-single-column'
    if ($firstMatch.search_matches -gt 1) {
        Keys '{ENTER}'
        $nextMatch = Snapshot 'tree-next-match'
        if ($nextMatch.active_match -ne 1 -or $nextMatch.selected_path -ne $nextMatch.match_path) { throw 'Next Enter did not advance to the second result' }
        Keys '+{ENTER}'
        $previousMatch = Snapshot 'tree-previous-match'
        if ($previousMatch.active_match -ne 0 -or $previousMatch.selected_path -ne $firstMatch.selected_path) { throw 'Shift Enter did not return to the first result' }
        Click 110 205
        $clickedMatch = Snapshot 'tree-clicked-match'
        if ($clickedMatch.active_match -ne 1 -or $clickedMatch.selected_path -ne $clickedMatch.match_path) { throw 'Mouse search selection differs from the second result' }
    }
    Keys '{ESC}'
    Keys 't'
    $off = Snapshot 'tree-tests-off'
    if ($off.tests) { throw 'Tree test toggle failed' }
    Click 225 35
    $back = Snapshot 'back-to-space'
    if ($back.mode -ne '12d' -or $back.tests -or $back.resident_bytes -ne $space.resident_bytes) { throw 'Switch back failed to preserve test filter and residency' }
    if ((Get-Item $report).LastWriteTime -ne $loadStamp) { throw 'Switch reloaded the source' }
    Keys 't'
    Keys 'f'
    Start-Sleep -Seconds 1
    $graphCamera = Snapshot 'graph-before-selection'
    Click 50 490
    $chosen = Snapshot 'graph-selected'
    if ($null -eq $chosen.selected -or -not (Same-Camera $graphCamera $chosen)) { throw 'Graph selection changed the fitted camera' }
    Shot '11-selected-neighborhood'
    Keys '{ENTER}'
    Start-Sleep -Seconds 1
    $reading = Snapshot 'space-reading'
    if (-not $reading.reading -or $null -eq $reading.selected_rect) { throw 'Read source did not open the selected card' }
    $card = $reading.selected_rect
    Click ($card.x + [Math]::Min(30, $card.w * 0.4)) ($card.y + [Math]::Min(80, $card.h * 0.4))
    Start-Sleep -Seconds 1
    $reselected = Snapshot 'space-reading-reselected'
    if ($reselected.selected -ne $reading.selected -or -not $reselected.reading -or -not (Same-Camera $reading $reselected)) { throw 'Reselecting the reading card changed selection or camera' }
    Shot '12-reading-reselected'
    Keys '{ESC}'
    Start-Sleep -Seconds 1
    $returned = Snapshot 'space-back-to-links'
    if ($returned.reading -or -not (Same-Camera $chosen $returned)) { throw 'Back to links did not restore the graph camera' }
    if ($null -ne $returned.first_reference) {
        Click 50 490
        $followed = Snapshot 'space-followed-reference'
        if ($followed.selected -ne $returned.first_reference.target -or -not (Same-Camera $returned $followed)) { throw 'Following a reference lost the expected endpoint or graph framing' }
        Shot '13-followed-reference'
    }
    Keys '{ENTER}'
    Start-Sleep -Seconds 1
    Shot '04-space-read-single-column'
    Keys ']'
    Start-Sleep -Seconds 2
    $fit = Snapshot 'rotation-after-read'
    if ($fit.outside_files -ne 0) { throw 'Rotation after reading escaped the viewport' }
    Shot '05-rotation-after-read'
    # Non-adjacent, disjoint and shared-axis transitions across all twelve axes.
    foreach ($pair in @(@(0,11),@(7,8),@(2,5),@(9,11),@(0,1),@(4,10),@(1,6),@(3,9))) {
        Click (49 + $pair[1]*20 + 8) (163 + $pair[0]*20 + 8)
        Start-Sleep -Milliseconds 1600
    }
    # Interrupt an in-flight rotation and redirect twice.
    Keys ']'
    Start-Sleep -Milliseconds 400
    Keys ']'
    Start-Sleep -Milliseconds 400
    Keys ']'
    Start-Sleep -Seconds 2
    $fit = Snapshot 'interrupted'
    if ($fit.outside_files -ne 0) { throw 'Interrupted rotation escaped viewport' }
    $width = $fit.logical_size[0]
    $height = $fit.logical_size[1]
    Click (621 + ($width - 880) * 0.5) ($height - 32)
    $mid = Snapshot 'midpoint'
    if ($mid.t -lt 0.45 -or $mid.t -gt 0.55 -or $mid.outside_files -ne 0) { throw 'Scrub framing failed' }
    Shot '06-midpoint'
    Keys 't'
    $midOff = Snapshot 'midpoint-tests-off'
    if ($midOff.outside_files -ne 0) { throw 'Test switch lost rotation framing' }
    Shot '07-midpoint-tests-off'
    [CodeBushWindow]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 100, 100, 1850, 1320, 0) | Out-Null
    Start-Sleep -Seconds 1
    $small = Snapshot 'resized'
    if ($small.outside_files -ne 0) { throw 'Resize lost framing' }
    Shot '08-resized'
    Click 225 35
    $treeAgain = Snapshot 'tree-return'
    if ($treeAgain.mode -ne 'tree' -or $treeAgain.tests) { throw 'Repeated GUI toggle failed' }
    if (-not $treeAgain.selected_visible -or $treeAgain.selected_path -ne $off.selected_path) { throw 'Test filtering lost the selected source while switching views' }
    Shot '09-tree-return'
    Click 225 35
    Keys ' '
    Start-Sleep -Seconds 5
    Keys ' '
    $end = Snapshot 'tour-end'
    if ($end.outside_files -ne 0) { throw 'Tour framing failed' }
    Shot '10-tour-end'
} finally {
    if (-not $process.HasExited) {
        $process.CloseMainWindow() | Out-Null
        if (-not $process.WaitForExit(30000)) { throw 'Viewer did not close' }
    }
}
$timing = Get-Content "$Output\native-timing.json" -Raw | ConvertFrom-Json
$rotating = @($timing.frames | Where-Object { $_.mode -eq '12d' -and $_.fitted -and -not $_.framing -and $_.animating -and $_.rotation_t -gt 0 -and $_.rotation_t -lt 1 })
$escaped = @($rotating | Where-Object { $_.outside_files -ne 0 })
if ($rotating.Count -lt 100) { throw 'Insufficient actual rotation frames' }
if ($escaped.Count -gt 0) { throw "Clipped source in $($escaped.Count) rotation frames" }
if ($timing.rendering_errors.Count -gt 0) { throw 'GPU validation errors' }
@{success=$true; source=$Source; initial=$initial; total_frames=$timing.frames.Count; rotating_frames=$rotating.Count; escaped_frames=$escaped.Count; rendering_errors=$timing.rendering_errors; shared_residency=$true; gui_mode_round_trips=3; cli_tree_start=[bool]$TreeStart} | ConvertTo-Json -Depth 8 | Set-Content "$Output\result.json"
Get-Content "$Output\result.json"
