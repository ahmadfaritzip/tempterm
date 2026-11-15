Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots\workflows"
$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Zoom {
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

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static void ClickAt(int x, int y) {
        SetCursorPos(x, y);
        System.Threading.Thread.Sleep(80);
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        System.Threading.Thread.Sleep(80);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
        System.Threading.Thread.Sleep(150);
    }
}
"@
Add-Type -TypeDefinition $csharp -ErrorAction SilentlyContinue

function Capture-Window($hwnd, $outFile) {
    [Win32Zoom]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Zoom]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Zoom+RECT
    [Win32Zoom]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $gfx = [System.Drawing.Graphics]::FromImage($bmp)
    $gfx.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
    $bmp.Save($outFile, [System.Drawing.Imaging.ImageFormat]::Png)
    $gfx.Dispose()
    $bmp.Dispose()
    Write-Host "Captured: $outFile"
}

# Stop any running instances
Get-Process -Name "temp-terminal-gui" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

# Launch temp-terminal-gui
$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
$proc = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 3

$hwnd = $proc.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd = (Get-Process -Id $proc.Id).MainWindowHandle
}

[Win32Zoom]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

$rect = New-Object Win32Zoom+RECT
[Win32Zoom]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

# Split terminal right (Ctrl+Shift+R)
[System.Windows.Forms.SendKeys]::SendWait("^+R")
Start-Sleep -Milliseconds 800

# Click Zoom on the left pane (around rect.Left + 745, rect.Top + 85)
Write-Host "Clicking Zoom button on left pane at X=$($rect.Left + 745), Y=$($rect.Top + 85)..."
[Win32Zoom]::ClickAt($rect.Left + 745, $rect.Top + 85)
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\wf_04_pane_zoomed.png"

# Click Restore (same location)
Write-Host "Clicking Restore button..."
[Win32Zoom]::ClickAt($rect.Left + 745, $rect.Top + 85)
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\wf_05_pane_restored.png"

Stop-Process -Id $proc.Id -Force
Write-Host "Zoom test complete."
