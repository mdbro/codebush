param(
    [string]$Source = '\\wsl.localhost\Ubuntu\home\mdwbr\codebush',
    [string]$OpenSource = '\\wsl.localhost\Ubuntu\dev\shm\codetree-benchmarks\ripgrep',
    [string]$Query = 'camera.rs',
    [string]$Output = 'D:\CodeBush\evidence\native-smoke'
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
$report = 'D:\CodeBush\evidence\latest-load.json'
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
    [CodeBushWindow]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 100, 100, 1920, 1280, 0) | Out-Null
    [CodeBushWindow]::AttachThreadInput($currentThread, $foregroundThread, $false) | Out-Null
    Start-Sleep -Seconds 1
    Keys 'f'
    Shot '01-overview'
    Keys '^k'
    Keys $Query
    Start-Sleep -Seconds 2
    Shot '02a-search-overview'
    Keys '{ENTER}'
    Shot '02-search'
    Keys '{ESC}'
    Shot '03-read'
    Keys 't'
    Keys 'f'
    Shot '04-tests-hidden'
    Keys 't'
    $before = Get-Date
    Keys '^r'
    $reload = Wait-Report $before
    Copy-Item $report "$Output\reload.json"
    if ($reload.resident_bytes -lt $initial.resident_bytes * 0.9) { throw 'Reload lost the resident cache' }
    Shot '05-reloaded'
    $before = Get-Date
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
    if (-not $opened.root.EndsWith((Split-Path $OpenSource -Leaf))) { throw 'Folder picker did not open the requested repository' }
    Shot '06-opened-folder'
    @{ success = $true; initial = $initial; reload = $reload; opened = $opened } | ConvertTo-Json -Depth 10 | Set-Content "$Output\result.json"
} finally {
    if (-not $process.HasExited) {
        $process.CloseMainWindow() | Out-Null
        if (-not $process.WaitForExit(30000)) { throw 'Viewer did not finish closing' }
    }
}
