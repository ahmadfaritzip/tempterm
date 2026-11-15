Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline\group-e"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32GroupE {
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
    [Win32GroupE]::ShowWindow($hwnd, 9) | Out-Null
    [Win32GroupE]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32GroupE+RECT
    [Win32GroupE]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

Write-Host "Setting up test workspace with sample files..."
$dirTest = "D:\temp_workspace_editor"
if (-not (Test-Path $dirTest)) { New-Item -ItemType Directory -Path $dirTest -Force | Out-Null }
$srcDir = Join-Path $dirTest "src"
if (-not (Test-Path $srcDir)) { New-Item -ItemType Directory -Path $srcDir -Force | Out-Null }

$mainRs = @"
// temp-terminal: high-performance Rust terminal workspace
use std::collections::HashMap;

pub struct TerminalWorkspace {
    pub name: String,
    pub session_count: usize,
    pub active: bool,
}

impl TerminalWorkspace {
    pub fn new(name: &str) -> Self {
        println!("Initializing workspace: {}", name);
        Self {
            name: name.to_string(),
            session_count: 1,
            active: true,
        }
    }

    pub fn summary(&self) -> String {
        format!("Workspace '{}' with {} sessions", self.name, self.session_count)
    }
}

fn main() {
    let ws = TerminalWorkspace::new("temp-terminal");
    println!("Workspace active: {}", ws.active);
}
"@
[System.IO.File]::WriteAllText((Join-Path $srcDir "main.rs"), $mainRs)

$notesXyz = @"
Plain text document without recognized language extension.
Testing fallback to plain text rendering without error or syntax highlights.
All characters appear with uniform neutral foreground.
"@
[System.IO.File]::WriteAllText((Join-Path $dirTest "notes.xyz"), $notesXyz)

# Seed state.json pointing to this workspace
$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
$statePath = Join-Path $configDir "state.json"
$settingsPath = Join-Path $configDir "settings.json"

$seedState = @{
    version = 2
    viewed = "proj_editor"
    workspace = $dirTest
    projects = @(
        @{
            id = "proj_editor"
            name = "Editor-Workspace"
            dir = $dirTest
        }
    )
}
$seedJson = $seedState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $seedJson)

# Ensure default editor settings
$seedSettings = @{
    theme = "dark"
    terminal = @{
        fontSize = 13.0
        cursorBlink = $true
    }
    editor = @{
        tabSize = 4
        wrap = $true
        lineNumbers = $true
        diffSplit = $true
    }
}
$settingsJson = $seedSettings | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($settingsPath, $settingsJson)

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

[Win32GroupE]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Open main.rs via F8
Write-Host "1. Opening main.rs with syntax highlighting..."
[System.Windows.Forms.SendKeys]::SendWait("{F8}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "group_e_01_editor_syntax_highlighting.png")

# 2. Line numbers gutter
Write-Host "2. Capturing editor with line numbers gutter..."
Capture-Window $hwnd (Join-Path $outDir "group_e_02_editor_line_numbers_gutter.png")

# 3. Word wrap enabled
Write-Host "3. Capturing editor with word wrap enabled..."
Capture-Window $hwnd (Join-Path $outDir "group_e_03_editor_word_wrap_enabled.png")

# 4. Mark buffer dirty via F10
Write-Host "4. Modifying buffer (dirty state)..."
[System.Windows.Forms.SendKeys]::SendWait("{F10}")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "group_e_04_editor_modified_dirty_buffer.png")

# 5. Diff viewer in split mode via F7
Write-Host "5. Opening Diff viewer (Split mode)..."
[System.Windows.Forms.SendKeys]::SendWait("{F7}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "group_e_05_editor_diff_split_view.png")

# 6. Toggle Diff view to Inline mode
Write-Host "6. Toggling Diff viewer to Inline mode..."
# Click Inline button in diff toolbar
$rect = New-Object Win32GroupE+RECT
[Win32GroupE]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
# Right toolbar area
[Win32GroupE]::ClickAt($rect.Right - 80, $rect.Top + 62)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "group_e_06_editor_diff_inline_view.png")

# Close diff view via Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

# 7. Save file via F12
Write-Host "7. Saving file (clean state)..."
[System.Windows.Forms.SendKeys]::SendWait("{F12}")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "group_e_07_editor_after_saving_file.png")

# 8. Open unsupported extension (notes.xyz) via F8
Write-Host "8. Opening notes.xyz with plain-text fallback..."
[System.Windows.Forms.SendKeys]::SendWait("{F8}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "group_e_08_editor_unsupported_ext_plain_text.png")

# Switch back to main.rs
[System.Windows.Forms.SendKeys]::SendWait("{F8}")
Start-Sleep -Milliseconds 400

# 9. Dark theme (current)
Write-Host "9. Editor in Dark theme..."
Capture-Window $hwnd (Join-Path $outDir "group_e_09_editor_dark_theme.png")

# 10. Switch to Light theme via F9
Write-Host "10. Switching to Light theme..."
[System.Windows.Forms.SendKeys]::SendWait("{F9}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "group_e_10_editor_light_theme.png")

# 11. Switch to Omarchy theme via F9
Write-Host "11. Switching to Omarchy theme..."
[System.Windows.Forms.SendKeys]::SendWait("{F9}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "group_e_11_editor_omarchy_theme.png")

# Switch back to Dark theme
[System.Windows.Forms.SendKeys]::SendWait("{F9}")
Start-Sleep -Milliseconds 400

# 12. Mark dirty (F10) then try close (F11) to trigger Unsaved Changes modal
Write-Host "12. Triggering Unsaved Changes confirmation modal..."
[System.Windows.Forms.SendKeys]::SendWait("{F10}")
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("{F11}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "group_e_12_editor_unsaved_confirm_modal.png")

# Dismiss modal with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 400

Write-Host "Group E acceptance capture complete! Terminating process..."
Stop-Process -Id $p.Id -Force
Write-Host "Done."
