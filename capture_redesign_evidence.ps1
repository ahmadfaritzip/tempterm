Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Redesign {
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
    [Win32Redesign]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Redesign]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Redesign+RECT
    [Win32Redesign]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top

    if ($w -le 0 -or $h -le 0) {
        Write-Host "Invalid window dimensions: $w x $h"
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

# Clean any existing temp-terminal GUI processes
Get-Process -Name "temp-terminal-gui" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

# Start temp-terminal GUI
$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
$proc = Start-Process -FilePath $exe -PassThru

# Wait for window handle
$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Milliseconds 200
    $proc.Refresh()
    if ($proc.MainWindowHandle -ne [IntPtr]::Zero) {
        $hwnd = $proc.MainWindowHandle
        break
    }
}

if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "Failed to obtain temp-terminal MainWindowHandle"
    exit 1
}

# Position window at 1440x900
[Win32Redesign]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 600

# 1. Main Initial Redesigned Workspace
Capture-Window $hwnd "$outDir\redesign_01_main_workspace.png"

# 2. Open a new terminal session (Ctrl+T)
[System.Windows.Forms.SendKeys]::SendWait("^t")
Start-Sleep -Milliseconds 1200
Capture-Window $hwnd "$outDir\redesign_02_terminal_session.png"

# 3. Open Keyboard Shortcuts Overlay (F1)
[System.Windows.Forms.SendKeys]::SendWait("{F1}")
Start-Sleep -Milliseconds 1000
Capture-Window $hwnd "$outDir\redesign_03_shortcuts_overlay.png"

# Close Shortcuts Overlay (Esc)
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 500

# 4. Open Command Palette (Ctrl+P)
[System.Windows.Forms.SendKeys]::SendWait("^p")
Start-Sleep -Milliseconds 1000
Capture-Window $hwnd "$outDir\redesign_04_command_palette.png"

# Close Command Palette (Esc)
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 500

# 5. Open Inspector (Ctrl+Shift+B)
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 1000
Capture-Window $hwnd "$outDir\redesign_05_inspector_open.png"

# Clean up process
Stop-Process -Id $proc.Id -Force
Write-Host "Redesign capture sequence complete."
