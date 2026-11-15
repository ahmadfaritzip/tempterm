Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-redesign\screenshots\workflows"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Workflow {
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
    [Win32Workflow]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Workflow]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Workflow+RECT
    [Win32Workflow]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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
if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "Failed to get MainWindowHandle"
    exit 1
}

[Win32Workflow]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

$rect = New-Object Win32Workflow+RECT
[Win32Workflow]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

# Workflow 1: Interactive Terminal Input & Execution
Write-Host "Workflow 1: Interactive Terminal Input (Get-Date)..."
# Click inside terminal area
[Win32Workflow]::ClickAt($rect.Left + 500, $rect.Top + 300)
Start-Sleep -Milliseconds 400

[System.Windows.Forms.SendKeys]::SendWait("Write-Host 'Linear Slate Redesign Verified!'{ENTER}")
Start-Sleep -Milliseconds 1200
Capture-Window $hwnd "$outDir\wf_01_terminal_command_executed.png"

# Workflow 2: Split Panes and Focus Navigation
Write-Host "Workflow 2: Split Panes and Focus..."
[System.Windows.Forms.SendKeys]::SendWait("^+R")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\wf_02_split_right_active.png"

# Focus left pane
[Win32Workflow]::ClickAt($rect.Left + 400, $rect.Top + 400)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd "$outDir\wf_03_switch_focus_left.png"

# Workflow 3: Zoom and Restore Pane
Write-Host "Workflow 3: Zoom / Restore..."
[Win32Workflow]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^+Z")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\wf_04_pane_zoomed.png"

[Win32Workflow]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("^+Z")
Start-Sleep -Milliseconds 800
Capture-Window $hwnd "$outDir\wf_05_pane_restored.png"

# Close right split pane
[System.Windows.Forms.SendKeys]::SendWait("^w")
Start-Sleep -Milliseconds 600

# Workflow 4: Command Palette
Write-Host "Workflow 4: Command Palette..."
# Click on ⌘ P button at top right (rect.Left + 1370, rect.Top + 55)
# Or trigger via shortcut
[System.Windows.Forms.SendKeys]::SendWait("^+P")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\wf_06_command_palette.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# Workflow 5: Settings Master-Detail Modal
Write-Host "Workflow 5: Settings Master-Detail..."
[System.Windows.Forms.SendKeys]::SendWait("^,")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\wf_07_settings_general.png"

# Click Appearance category in Settings sidebar (approx rect.Left + 410, rect.Top + 265)
[Win32Workflow]::ClickAt($rect.Left + 410, $rect.Top + 265)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\wf_08_settings_appearance.png"

# Click Terminal category (approx rect.Left + 410, rect.Top + 290)
[Win32Workflow]::ClickAt($rect.Left + 410, $rect.Top + 290)
Start-Sleep -Milliseconds 400
Capture-Window $hwnd "$outDir\wf_09_settings_terminal.png"

[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# Workflow 6: Keyboard Shortcuts Overlay
Write-Host "Workflow 6: Shortcuts Overlay..."
[System.Windows.Forms.SendKeys]::SendWait("{F1}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\wf_10_shortcuts_overlay.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# Workflow 7: New Agent Modal
Write-Host "Workflow 7: New Agent Modal..."
[System.Windows.Forms.SendKeys]::SendWait("^+A")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\wf_11_new_agent_modal.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# Workflow 8: Add Process Modal
Write-Host "Workflow 8: Add Process Modal..."
[System.Windows.Forms.SendKeys]::SendWait("^+N")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\wf_12_add_process_modal.png"
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# Workflow 9: Inspector Panel
Write-Host "Workflow 9: Inspector Panel..."
[System.Windows.Forms.SendKeys]::SendWait("^+B")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd "$outDir\wf_13_inspector_panel.png"

Stop-Process -Id $proc.Id -Force
Write-Host "Core workflows test suite completed."
