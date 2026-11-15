Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32GroupD {
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
Add-Type -TypeDefinition $csharp

function Capture-Window($hwnd, $outFile) {
    [Win32GroupD]::ShowWindow($hwnd, 9) | Out-Null
    [Win32GroupD]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32GroupD+RECT
    [Win32GroupD]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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
$dirTest = "D:\temp_workspace_dialogs"
if (-not (Test-Path $dirTest)) { New-Item -ItemType Directory -Path $dirTest -Force | Out-Null }

$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
$statePath = Join-Path $configDir "state.json"

$seedState = @{
    version = 2
    viewed = "proj_dialogs"
    workspace = $dirTest
    projects = @(
        @{
            id = "proj_dialogs"
            name = "Dialogs-Workspace"
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

[Win32GroupD]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

$rect = New-Object Win32GroupD+RECT
[Win32GroupD]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

# 1. New Actions Menu Dropdown
Write-Host "1. Capturing New Actions Menu Dropdown..."
# Top chevron ▾ is near X = rect.Left + 258, Y = rect.Top + 24
[Win32GroupD]::ClickAt($rect.Left + 258, $rect.Top + 24)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd "$outDir\group_d_closure_01_new_action_menu_dropdown.png"

# Close dropdown with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 2. New Agent Modal Window (Ctrl+Shift+A)
Write-Host "2. Capturing New Agent Modal..."
[System.Windows.Forms.SendKeys]::SendWait("^+A")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_d_closure_02_new_agent_modal.png"

# Close agent modal with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 3. Add Process Modal Window (Ctrl+Shift+N)
Write-Host "3. Capturing Add Process Modal Window (Complete Form)..."
[System.Windows.Forms.SendKeys]::SendWait("^+N")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_d_closure_03_add_process_complete_form.png"

# Close process modal with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 4. Command Palette Modal Window (Ctrl+Shift+P)
Write-Host "4. Capturing Command Palette..."
[System.Windows.Forms.SendKeys]::SendWait("^+P")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_d_closure_04_command_palette.png"

# Close command palette with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 5. Quick Open File Picker (Ctrl+P)
Write-Host "5. Capturing Quick Open File Picker..."
[System.Windows.Forms.SendKeys]::SendWait("^p")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_d_closure_05_quick_open_file_picker.png"

# Close quick open with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 6. Keyboard Shortcuts Overlay (F1)
Write-Host "6. Capturing Keyboard Shortcuts Overlay..."
[System.Windows.Forms.SendKeys]::SendWait("{F1}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\group_d_closure_06_shortcuts_overlay.png"

# Close shortcuts overlay with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 7. Open Settings Modal (Ctrl+,)
Write-Host "7. Opening Settings Modal Window..."
[System.Windows.Forms.SendKeys]::SendWait("^,")
Start-Sleep -Milliseconds 600

# Window center is ~ 720, 450.
# Left sidebar of settings modal starts at X ~ rect.Left + 360 to 500, Y ~ rect.Top + 220 to 600.
# Let's capture General:
Write-Host "Capturing Settings: General..."
Capture-Window $hwnd "$outDir\group_d_closure_07_settings_general.png"

# Let's click category buttons in Settings modal:
# Modal is 760x520 centered. Left is rect.Left + (1440 - 760)/2 = rect.Left + 340.
# Top is rect.Top + (900 - 520)/2 = rect.Top + 190.
# Sidebar buttons start at X = rect.Left + 410.
# Y starts around rect.Top + 230 and increments by 26px:
# General: Y ~ 235
# Appearance: Y ~ 261
# Terminal: Y ~ 287
# Editor: Y ~ 313
# Notifications: Y ~ 339
# Agents: Y ~ 365
# Web Access: Y ~ 391
# Dev URLs: Y ~ 417
# Browser: Y ~ 443
# Keyboard: Y ~ 469
# About: Y ~ 495

$catX = $rect.Left + 410

# Category: Appearance
[Win32GroupD]::ClickAt($catX, $rect.Top + 261)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_08_settings_appearance.png"

# Category: Terminal
[Win32GroupD]::ClickAt($catX, $rect.Top + 287)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_09_settings_terminal.png"

# Category: Agents
[Win32GroupD]::ClickAt($catX, $rect.Top + 365)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_10_settings_agents.png"

# Category: Web Access
[Win32GroupD]::ClickAt($catX, $rect.Top + 391)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_11_settings_web_access_windows.png"

# Category: Dev URLs
[Win32GroupD]::ClickAt($catX, $rect.Top + 417)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_12_settings_devurls_windows.png"

# Category: Browser
[Win32GroupD]::ClickAt($catX, $rect.Top + 443)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_13_settings_browser.png"

# Category: Keyboard
[Win32GroupD]::ClickAt($catX, $rect.Top + 469)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_14_settings_keyboard.png"

# Category: About
[Win32GroupD]::ClickAt($catX, $rect.Top + 495)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\group_d_closure_15_settings_about.png"

# Close settings with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# Close application cleanly
Write-Host "Closing application..."
Stop-Process -Id $p.Id -Force
Start-Sleep -Milliseconds 500

Write-Host "Group D acceptance capture completed successfully!"
