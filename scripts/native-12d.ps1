param(
    [string]$Source = '\\wsl.localhost\Ubuntu\home\mdwbr\codebush',
    [string]$OpenSource = '\\wsl.localhost\Ubuntu\dev\shm\codetree-benchmarks\ripgrep',
    [string]$Query = 'camera.rs',
    [string]$Output = 'D:\CodeBush\evidence\12d-native-4',
    [switch]$ReloadAndOpen
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
$report = 'D:\CodeBush\evidence\12d-latest\load-report.json'
$started = Get-Date
$process = Start-Process 'D:\CodeBush\bin\codebush.exe' -ArgumentList ('"' + $Source + '" --native-timing "' + $Output + '\native-timing.json"') -WorkingDirectory 'D:\CodeBush' -PassThru -RedirectStandardOutput "$Output\stdout.log" -RedirectStandardError "$Output\stderr.log"
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
    Start-Sleep -Milliseconds 500
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
    Keys 'f'
    Shot '01-connected-plane'
    Click 50 490
    Keys '{ENTER}'
    Shot '02-read-source'
    Click 245 455
    Shot '08-reference-evidence'
    Click 50 498
    Keys '{ENTER}'
    Shot '09-followed-reference'
    Click 245 455
    Keys 'f'
    Keys ']'
    Start-Sleep -Seconds 2
    Shot '03-next-plane'
    $expectedPlane = Snapshot 'before-scrub'
    $rect = New-Object CodeBushWindow+Rect
    [CodeBushWindow]::GetClientRect($process.MainWindowHandle, [ref]$rect) | Out-Null
    $dpi = [CodeBushWindow]::GetDpiForWindow($process.MainWindowHandle) / 96.0
    $width = ($rect.right - $rect.left) / $dpi
    $height = ($rect.bottom - $rect.top) / $dpi
    Click (621 + ($width - 880) * 0.5) ($height - 32)
    Start-Sleep -Seconds 1
    Shot '04-held-mid-rotation'
    $overlap = Snapshot 'overlap-start'
    $clickedMid = $overlap
    if ($clickedMid.plane -ne $expectedPlane.plane -or $clickedMid.t -lt 0.45 -or $clickedMid.t -gt 0.55 -or $clickedMid.animating) { throw 'Clicked midpoint state is incorrect' }
    if ($null -eq $overlap.hotspot -or $overlap.candidates.Count -lt 2) { throw 'No visible overlap fixture found' }
    Click $overlap.hotspot[0] $overlap.hotspot[1]
    $visited = @()
    for ($candidate=0; $candidate -lt $overlap.candidates.Count; $candidate++) {
        $picked = Snapshot "overlap-$candidate"
        $visited += $picked.selected
        [CodeBushWindow]::keybd_event(16,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
        Click $overlap.hotspot[0] $overlap.hotspot[1]
        [CodeBushWindow]::keybd_event(16,0,2,[UIntPtr]::Zero)
    }
    if (@($visited | Select-Object -Unique).Count -ne $overlap.candidates.Count) { throw 'Shift-click did not visit every overlapping file' }
    Keys 'o'
    $listed = Snapshot 'overlap-list'
    if ($listed.candidate_list.Count -ne $overlap.candidates.Count) { throw 'Candidate list lost files' }
    Shot '13-overlap-candidates'
    Keys '{ESC}'
    $beforeFollow = Snapshot 'before-follow'
    Click 245 455
    $evidence = Snapshot 'evidence-open'
    if (-not $evidence.relations) { throw 'Reference evidence did not open' }
    Shot '14-reference-evidence'
    Click 50 498
    $followed = Snapshot 'after-follow'
    if ($followed.selected -ne $beforeFollow.first_reference.target) { throw 'Reference row selected the wrong file' }
    Keys '{ENTER}'
    Shot '15-followed-reference'
    Click 245 455
    Keys 'f'
    Click (621 + ($width - 880) * 0.5) ($height - 32)
    Start-Sleep -Seconds 1
    $point = New-Object CodeBushWindow+Point
    [CodeBushWindow]::ClientToScreen($process.MainWindowHandle, [ref]$point) | Out-Null
    [CodeBushWindow]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    for ($step=1; $step -le 8; $step++) {
        $x = 621 + ($width-880) * (0.5 + 0.22*$step/8)
        [CodeBushWindow]::SetCursorPos([int]($point.x+$x*$dpi), [int]($point.y+($height-32)*$dpi)) | Out-Null
        Start-Sleep -Milliseconds 50
    }
    [CodeBushWindow]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Seconds 1
    Shot '10-dragged-rotation'
    $dragState = Snapshot 'dragged'
    if ($dragState.plane -ne $expectedPlane.plane -or $dragState.t -lt 0.68 -or $dragState.t -gt 0.76 -or $dragState.animating) { throw 'Dragged state is incorrect' }
    Keys 't'
    Keys 'f'
    Shot '05-tests-hidden'
    Keys 't'
    Keys ' '
    Start-Sleep -Seconds 15
    Keys ' '
    Shot '06-tour'
    Keys '?'
    Shot '07-help'
    Keys '{ESC}'
    if ($ReloadAndOpen) {
        $before = Get-Date
        $script:LoadingShot = '16-loading-reload'
        Keys '^r'
        $reload = Wait-Report $before
        Copy-Item $report "$Output\reload.json"
        if ($reload.resident_bytes -lt $initial.resident_bytes * 0.9) { throw 'Reload lost source residency' }
        Shot '11-reloaded'
        $before = Get-Date
        $script:LoadingShot = '17-loading-open'
        Keys '^o'
        Start-Sleep -Seconds 1
        Keys '^l'
        Keys $OpenSource
        Keys '{ENTER}'
        Start-Sleep -Seconds 1
        $dialog = [CodeBushWindow]::GetForegroundWindow()
        $owner = [uint32]0
        [CodeBushWindow]::GetWindowThreadProcessId($dialog, [ref]$owner) | Out-Null
        if ($owner -ne $process.Id) { throw 'Folder picker lost focus' }
        $select = [CodeBushWindow]::GetDlgItem($dialog, 1)
        if ($select -eq [IntPtr]::Zero) { throw 'Folder picker has no Select Folder button' }
        [CodeBushWindow]::SendMessage($select, 245, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        $opened = Wait-Report $before
        Copy-Item $report "$Output\opened-folder.json"
        if (-not $opened.root.EndsWith((Split-Path $OpenSource -Leaf))) { throw 'Wrong folder opened' }
        Shot '12-opened-folder'
    }

} finally {
    if (-not $process.HasExited) {
        $process.CloseMainWindow() | Out-Null
        if (-not $process.WaitForExit(30000)) { throw 'Viewer did not close' }
    }
}

$timing = Get-Content "$Output\native-timing.json" -Raw | ConvertFrom-Json
$held = @($timing.frames | Where-Object { $_.command_seq -eq $clickedMid.command_seq -and $_.rotation_t -gt 0.4 -and $_.rotation_t -lt 0.6 -and -not $_.animating })
$dragged = @($timing.frames | Where-Object { $_.command_seq -eq $dragState.command_seq -and $_.rotation_t -gt 0.68 -and $_.rotation_t -lt 0.76 -and -not $_.animating })
if ($dragged.Count -lt 60) { throw 'Drag did not hold its intermediate orientation' }
if ($held.Count -lt 60) { throw "No verified held midpoint: $($held.Count) frames" }
if ($timing.rendering_errors.Count -ne 0) { throw 'Rendering errors' }
$loadingFrames = @($timing.frames | Where-Object { $_.loading })
if ($ReloadAndOpen -and $loadingFrames.Count -lt 60) { throw 'No responsive loading frames captured' }
@{success=$true;initial=$initial;held_midpoint_frames=$held.Count;dragged_frames=$dragged.Count;loading_frames=$loadingFrames.Count;reload=$reload;opened=$opened;cycled_ids=$visited;expected_overlap_ids=$overlap.candidates;followed_id=$followed.selected;expected_followed_id=$beforeFollow.first_reference.target} | ConvertTo-Json -Depth 8 | Set-Content "$Output\result.json"
