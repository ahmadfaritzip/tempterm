Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Capture {
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
    [Win32Capture]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Capture]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Capture+RECT
    [Win32Capture]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

# Seed clean state with no sessions so we can see the redesigned empty state
$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
$statePath = Join-Path $configDir "state.json"

$seedState = @{
    version = 2
    viewed = "proj_redesign"
    workspace = "D:\project\myproject\rewrite-terminal"
    projects = @(
        @{
            id = "proj_redesign"
            name = "rewrite-terminal"
            dir = "D:\project\myproject\rewrite-terminal"
        }
    )
}
$seedJson = $seedState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $seedJson)

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
    Write-Error "Failed to obtain temp-terminal MainWindowHandle"
    exit 1
}

# Position window at 1440x900
[Win32Capture]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

$rect = New-Object Win32Capture+RECT
[Win32Capture]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

# 1. Main Initial Redesigned Workspace (Empty state with action cards)
Capture-Window $hwnd "$outDir\redesign_01_main_workspace.png"

# 2. Open a new terminal session (Ctrl+T)
[System.Windows.Forms.SendKeys]::SendWait("^t")
Start-Sleep -Milliseconds 1200
Capture-Window $hwnd "$outDir\redesign_02_terminal_session.png"

# 3. Open Command Palette (Ctrl+Shift+P)
[System.Windows.Forms.SendKeys]::SendWait("^+P")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\redesign_03_command_palette.png"

# Close Command Palette (Esc)
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 4. Open Settings Modal Window (Ctrl+,)
[System.Windows.Forms.SendKeys]::SendWait("^,")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\redesign_04_settings_modal.png"

# Close Settings Modal (Esc)
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 5. Open Keyboard Shortcuts Overlay (F1)
[System.Windows.Forms.SendKeys]::SendWait("{F1}")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\redesign_05_shortcuts_overlay.png"

# Close Shortcuts Overlay (Esc)
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 6. Open New Agent Modal (Ctrl+Shift+A)
[System.Windows.Forms.SendKeys]::SendWait("^+A")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\redesign_06_new_agent_modal.png"

# Close Agent Modal (Esc)
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 7. Open Right Inspector (Ctrl+Shift+B)
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\redesign_07_inspector_panel.png"

# Clean up process
Stop-Process -Id $proc.Id -Force
Write-Host "Redesign capture suite complete."
