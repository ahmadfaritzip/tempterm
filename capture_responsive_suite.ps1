Add-Type -AssemblyName System.Drawing

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32CaptureSuite {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBkgnd, uint nFlags);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

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

$respDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots\responsive"
if (-not (Test-Path $respDir)) {
    New-Item -ItemType Directory -Path $respDir -Force | Out-Null
}

function Capture-WindowPrint($hwnd, $outFile) {
    [Win32CaptureSuite]::ShowWindow($hwnd, 9) | Out-Null
    Start-Sleep -Milliseconds 600

    $rect = New-Object Win32CaptureSuite+RECT
    [Win32CaptureSuite]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    if ($w -le 0 -or $h -le 0) {
        Write-Host "Invalid dimensions: $w x $h"
        return
    }

    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $gfx.GetHdc()
    $ok = [Win32CaptureSuite]::PrintWindow($hwnd, $hdc, 2)
    if (-not $ok) {
        [Win32CaptureSuite]::PrintWindow($hwnd, $hdc, 0) | Out-Null
    }
    $gfx.ReleaseHdc($hdc)
    $gfx.Dispose()

    $bmp.Save($outFile, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $len = (Get-Item $outFile).Length
    Write-Host "Captured: $outFile ($w x $h, $len bytes)"
}

# Clean any existing GUI
Get-Process -Name "temp-terminal-gui" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 400

# Start GUI
$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
$proc = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 2

$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 15; $i++) {
    $p = Get-Process -Name "temp-terminal-gui" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($p -and $p.MainWindowHandle -ne [IntPtr]::Zero) {
        $hwnd = $p.MainWindowHandle
        break
    }
    Start-Sleep -Milliseconds 500
}

if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "Failed to obtain MainWindowHandle"
    exit 1
}

Write-Host "Window handle found: $hwnd"

# 1. 1920x1080 (Large Desktop)
[Win32CaptureSuite]::MoveWindow($hwnd, 0, 0, 1920, 1080, $true) | Out-Null
Start-Sleep -Milliseconds 800
Capture-WindowPrint $hwnd "$respDir\responsive_01_1920x1080.png"

# 2. 1440x900 (Standard Desktop)
[Win32CaptureSuite]::MoveWindow($hwnd, 40, 40, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800
Capture-WindowPrint $hwnd "$respDir\responsive_02_1440x900.png"

# 3. 1280x720 (Medium Desktop)
[Win32CaptureSuite]::MoveWindow($hwnd, 40, 40, 1280, 720, $true) | Out-Null
Start-Sleep -Milliseconds 800
Capture-WindowPrint $hwnd "$respDir\responsive_03_1280x720.png"

# 4. 1024x768 (Small Desktop)
[Win32CaptureSuite]::MoveWindow($hwnd, 40, 40, 1024, 768, $true) | Out-Null
Start-Sleep -Milliseconds 800
Capture-WindowPrint $hwnd "$respDir\responsive_04_1024x768.png"

# 5. 800x600 (Narrow Desktop)
[Win32CaptureSuite]::MoveWindow($hwnd, 40, 40, 800, 600, $true) | Out-Null
Start-Sleep -Milliseconds 800
Capture-WindowPrint $hwnd "$respDir\responsive_05_800x600.png"

# 6. 640x600 (Ultra-Narrow Desktop / Below 720px)
[Win32CaptureSuite]::MoveWindow($hwnd, 40, 40, 640, 600, $true) | Out-Null
Start-Sleep -Milliseconds 800
Capture-WindowPrint $hwnd "$respDir\responsive_06_640x600.png"

Stop-Process -Id $proc.Id -Force
Write-Host "Responsive capture completed."
