Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32GroupC {
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, int dwExtraInfo);

    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP   = 0x0004;
    public const uint MOUSEEVENTF_MOVE     = 0x0001;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static void DragMouse(int x1, int y1, int x2, int y2, int steps) {
        SetCursorPos(x1, y1);
        System.Threading.Thread.Sleep(100);
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        System.Threading.Thread.Sleep(100);

        for (int i = 1; i <= steps; i++) {
            int curX = x1 + (x2 - x1) * i / steps;
            int curY = y1 + (y2 - y1) * i / steps;
            SetCursorPos(curX, curY);
            System.Threading.Thread.Sleep(20);
        }

        System.Threading.Thread.Sleep(100);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
        System.Threading.Thread.Sleep(200);
    }
}
"@
Add-Type -TypeDefinition $csharp

function Capture-Window($hwnd, $outFile) {
    [Win32GroupC]::ShowWindow($hwnd, 9) | Out-Null
    [Win32GroupC]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32GroupC+RECT
    [Win32GroupC]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    if ($w -le 0 -or $h -le 0) {
        Write-Host "Invalid window dimensions: $w x $h"
        return
    }

    $size = New-Object System.Drawing.Size($w, $h)
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $size)
    $bmp.Save($outFile, [System.Drawing.Imaging.ImageFormat]::Png)
    $gfx.Dispose()
    $bmp.Dispose()
    Write-Host "Captured $outFile ($w x $h)"
}

Write-Host "Building latest temp-terminal-gui..."
Set-Location "D:\project\myproject\rewrite-terminal\temp-terminal"
cargo build -p temp-terminal-ui --bin temp-terminal-gui
if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!"
    exit 1
}

# Clean workspace state
$dirTest = "D:\temp_workspace_split"
if (-not (Test-Path $dirTest)) { New-Item -ItemType Directory -Path $dirTest -Force | Out-Null }

$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
$statePath = Join-Path $configDir "state.json"

$seedState = @{
    version = 2
    viewed = "proj_split"
    workspace = $dirTest
    projects = @(
        @{
            id = "proj_split"
            name = "Split-Workspace"
            dir = $dirTest
        }
    )
}
$seedJson = $seedState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $seedJson)
Write-Host "Seeded clean state at $statePath"

$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
Write-Host "Launching temp-terminal-gui..."
$p = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 3

$hwnd = $p.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd = (Get-Process -Id $p.Id).MainWindowHandle
}
if ($hwnd -eq [IntPtr]::Zero) {
    Write-Host "Window handle not found!"
    Stop-Process -Id $p.Id -Force
    exit 1
}

[Win32GroupC]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Split Terminal Right (Ctrl+Shift+R)
Write-Host "Creating Vertical Split (Ctrl+Shift+R)..."
[Win32GroupC]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^+R")
Start-Sleep -Seconds 1

Write-Host "1. Capturing Vertical Split before drag..."
Capture-Window $hwnd "$outDir\group_c_01_vertical_split_before_drag.png"

# 2. Drag vertical divider leftwards
$rect = New-Object Win32GroupC+RECT
[Win32GroupC]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
# Window width 1440, sidebar ~240, central ~1200. Divider is at X ~ 240 + 600 = 840
$divX = $rect.Left + 840
$divY = $rect.Top + 450
$targetX = $rect.Left + 660

Write-Host "Dragging vertical divider from $divX to $targetX..."
[Win32GroupC]::DragMouse($divX, $divY, $targetX, $divY, 20)
Start-Sleep -Seconds 1

Write-Host "2. Capturing Vertical Split after drag..."
Capture-Window $hwnd "$outDir\group_c_02_vertical_split_after_drag.png"

# 3. Create Horizontal Split in active right pane (Ctrl+Shift+D)
Write-Host "Creating Horizontal Split (Ctrl+Shift+D)..."
[Win32GroupC]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^+D")
Start-Sleep -Seconds 1

Write-Host "3. Capturing Horizontal Split before drag..."
Capture-Window $hwnd "$outDir\group_c_03_horizontal_split_dragged.png"

# Drag horizontal divider upwards
# Right side runs from X ~ 660 to 1440. Horizontal divider is at Y ~ 450
$hDivX = $rect.Left + 1050
$hDivY = $rect.Top + 465
$hTargetY = $rect.Top + 360
Write-Host "Dragging horizontal divider from $hDivY to $hTargetY..."
[Win32GroupC]::DragMouse($hDivX, $hDivY, $hDivX, $hTargetY, 15)
Start-Sleep -Seconds 1

Write-Host "4. Capturing Nested Splits layout..."
Capture-Window $hwnd "$outDir\group_c_04_nested_splits.png"

# 5. Toggle Zoom (Ctrl+Shift+Z)
Write-Host "Toggling Pane Zoom (Ctrl+Shift+Z)..."
[Win32GroupC]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^+Z")
Start-Sleep -Seconds 1

Write-Host "5. Capturing Zoomed Pane..."
Capture-Window $hwnd "$outDir\group_c_05_zoomed_pane.png"

# 6. Toggle Zoom off to restore split layout
Write-Host "Restoring Split Layout (Ctrl+Shift+Z)..."
[Win32GroupC]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^+Z")
Start-Sleep -Seconds 1

Write-Host "6. Capturing Restored Layout..."
Capture-Window $hwnd "$outDir\group_c_06_restored_layout.png"

# Close application cleanly
Write-Host "Closing application..."
Stop-Process -Id $p.Id -Force
Start-Sleep -Milliseconds 500

Write-Host "Group C acceptance capture completed successfully!"
