Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots\final"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Final {
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
    [Win32Final]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Final]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Final+RECT
    [Win32Final]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

# Start temp-terminal-gui
$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
$proc = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 3

$hwnd = $proc.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd = (Get-Process -Id $proc.Id).MainWindowHandle
}
if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "Failed to obtain MainWindowHandle"
    exit 1
}

# Position window at 1440x900
[Win32Final]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Main Terminal Session Workspace
Capture-Window $hwnd "$outDir\01_main_workspace_session.png"

# 2. Split Right Panes Workspace
[System.Windows.Forms.SendKeys]::SendWait("^+R")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\02_split_panes_workspace.png"

# Close right split pane
[System.Windows.Forms.SendKeys]::SendWait("^w")
Start-Sleep -Milliseconds 600

# 3. Command Palette Modal
[System.Windows.Forms.SendKeys]::SendWait("^+P")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\03_command_palette_modal.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 4. Settings Master-Detail Modal
[System.Windows.Forms.SendKeys]::SendWait("^,")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\04_settings_master_detail.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 5. Keyboard Shortcuts Overlay
[System.Windows.Forms.SendKeys]::SendWait("{F1}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\05_keyboard_shortcuts_overlay.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 6. New Agent Modal
[System.Windows.Forms.SendKeys]::SendWait("^+A")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\06_new_agent_modal.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 7. Add Process Modal
[System.Windows.Forms.SendKeys]::SendWait("^+N")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\07_add_process_modal.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 8. Inspector Panel Open
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\08_inspector_panel_open.png"
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 400

# Clean up process
Stop-Process -Id $proc.Id -Force
Write-Host "Final redesign capture sequence complete."
