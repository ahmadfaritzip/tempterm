Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots\responsive"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Resp {
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
Add-Type -TypeDefinition $csharp -ErrorAction SilentlyContinue

function Capture-Window($hwnd, $outFile) {
    [Win32Resp]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Resp]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Resp+RECT
    [Win32Resp]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    if ($w -le 0 -or $h -le 0) {
        Write-Host "Invalid dimensions: $w x $h"
        return
    }

    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
    $bmp.Save($outFile, [System.Drawing.Imaging.ImageFormat]::Png)
    $gfx.Dispose()
    $bmp.Dispose()
    Write-Host "Captured: $outFile ($w x $h)"
}

# Terminate running instance if any
Get-Process -Name "temp-terminal-gui" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

# Launch GUI
$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
$proc = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 3

$hwnd = $proc.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd = (Get-Process -Id $proc.Id).MainWindowHandle
}

# 1. Test 1024x768 (Compact)
[Win32Resp]::MoveWindow($hwnd, 40, 40, 1024, 768, $true) | Out-Null
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\responsive_01_1024x768.png"

# Split terminal down (Ctrl+Shift+D)
[System.Windows.Forms.SendKeys]::SendWait("^+D")
Start-Sleep -Milliseconds 800

# Split terminal right (Ctrl+Shift+R)
[System.Windows.Forms.SendKeys]::SendWait("^+R")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\responsive_02_1024x768_split_panes.png"

# 2. Test 1440x900 (Standard)
[Win32Resp]::MoveWindow($hwnd, 40, 40, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\responsive_03_1440x900_split_panes.png"

# 3. Test 1920x1080 (Full HD)
[Win32Resp]::MoveWindow($hwnd, 0, 0, 1920, 1080, $true) | Out-Null
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\responsive_04_1920x1080_split_panes.png"

Stop-Process -Id $proc.Id -Force
Write-Host "Responsive test suite completed."
