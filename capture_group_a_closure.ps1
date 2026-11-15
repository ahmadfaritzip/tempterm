Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Closure {
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
"@
Add-Type -TypeDefinition $csharp

function Capture-Window($hwnd, $outFile) {
    [Win32Closure]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Closure]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 300

    $rect = New-Object Win32Closure+RECT
    [Win32Closure]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
Write-Host "Launching temp-terminal-gui for Group A closure captures..."
$p = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 3

$hwnd = $p.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd = (Get-Process -Id $p.Id).MainWindowHandle
}

if ($hwnd -eq [IntPtr]::Zero) {
    Write-Host "Could not find window handle!"
    Stop-Process -Id $p.Id -Force
    exit 1
}

# Standardize window size to 1440x900
[Win32Closure]::MoveWindow($hwnd, 100, 100, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Main Application Shell with active terminal (default state)
Write-Host "Capturing 1. Main Shell..."
Capture-Window $hwnd "$outDir\group_a_closure_01_main_shell.png"

# 2. Inspector open (Ctrl+Shift+B)
Write-Host "Toggling Inspector open..."
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_a_closure_02_inspector_open.png"

# 3. Inspector Files tab (Ctrl+Shift+E)
Write-Host "Switching Inspector to Files tab..."
[System.Windows.Forms.SendKeys]::SendWait("^+E")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_a_closure_03_inspector_files.png"

# 4. Close Inspector (Ctrl+Shift+B) & Collapse Sidebar (Ctrl+B)
Write-Host "Closing Inspector and collapsing Sidebar..."
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^b")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_a_closure_04_sidebar_collapsed.png"

# 5. Expand Sidebar (Ctrl+B) & Add multiple session tabs (Ctrl+T twice)
Write-Host "Expanding Sidebar and adding session tabs..."
[System.Windows.Forms.SendKeys]::SendWait("^b")
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("^t")
Start-Sleep -Milliseconds 800
[System.Windows.Forms.SendKeys]::SendWait("^t")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\group_a_closure_05_multiple_session_tabs.png"

Write-Host "Stopping process..."
Stop-Process -Id $p.Id -Force
Write-Host "Group A closure screen capture completed successfully."
