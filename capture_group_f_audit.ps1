Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline\group-f"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$origDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline\original"

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Audit {
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
    [Win32Audit]::ShowWindow($hwnd, 9) | Out-Null
    [Win32Audit]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32Audit+RECT
    [Win32Audit]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

function Make-SideBySide($goPath, $rustPath, $outPath, $title) {
    if (-not (Test-Path $goPath) -or -not (Test-Path $rustPath)) {
        Write-Host "Skipping side-by-side (file not found): $goPath or $rustPath"
        return
    }
    $imgGo = [System.Drawing.Image]::FromFile($goPath)
    $imgRust = [System.Drawing.Image]::FromFile($rustPath)
    $w = 1440
    $h = 900
    $totalW = $w * 2 + 20
    $totalH = $h + 60
    $bmp = New-Object System.Drawing.Bitmap $totalW, $totalH
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear([System.Drawing.Color]::FromArgb(24, 24, 28))
    $font = New-Object System.Drawing.Font("Segoe UI", 16, [System.Drawing.FontStyle]::Bold)
    $brushGo = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(0, 180, 216))
    $brushRust = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(239, 108, 0))
    $g.DrawString("Original Go: $title", $font, $brushGo, 40, 15)
    $g.DrawString("Rust Rewrite: $title", $font, $brushRust, ($w + 60), 15)
    $g.DrawImage($imgGo, 0, 50, $w, $h)
    $g.DrawImage($imgRust, ($w + 20), 50, $w, $h)
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(60, 60, 70), 2)
    $g.DrawLine($pen, ($w + 10), 0, ($w + 10), $totalH)
    $bmp.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $pen.Dispose()
    $brushGo.Dispose()
    $brushRust.Dispose()
    $font.Dispose()
    $g.Dispose()
    $bmp.Dispose()
    $imgGo.Dispose()
    $imgRust.Dispose()
    Write-Host "Created side-by-side: $outPath"
}

Write-Host "Setting up test workspace for Group F parity audit..."
$dirTest = "D:\temp_workspace_audit"
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

$readmeMd = @"
# Temp Terminal Workspace

A high-performance Rust terminal workspace with full Go-to-Rust parity.

## Core Capabilities
- **Terminal Workspace**: Multi-split ConPTY panes with drag dividers and zoom
- **Inspector Panel**: Project Info, File Tree, and Git change tracking
- **In-Memory Editor**: Syntax highlighting, line numbers, word wrap, and live diff
- **Markdown Preview**: Formatted reading view with code blocks and headers

> Built with egui and Rust 1.83.
"@
[System.IO.File]::WriteAllText((Join-Path $dirTest "README.md"), $readmeMd)

# Seed state.json
$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
$statePath = Join-Path $configDir "state.json"
$settingsPath = Join-Path $configDir "settings.json"

$seedState = @{
    version = 2
    viewed = "proj_audit"
    workspace = $dirTest
    projects = @(
        @{
            id = "proj_audit"
            name = "Audit-Workspace"
            dir = $dirTest
        }
    )
}
$seedJson = $seedState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $seedJson)

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

[Win32Audit]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Main Application Window
Write-Host "Capturing 01: Main Application Window..."
Capture-Window $hwnd (Join-Path $outDir "rust_f01_main_window.png")

# 2. Sidebar Collapsed (Ctrl+B)
Write-Host "Capturing 02: Sidebar Collapsed..."
[System.Windows.Forms.SendKeys]::SendWait("^b")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f02_sidebar_collapsed.png")

# Expand sidebar back (Ctrl+B)
[System.Windows.Forms.SendKeys]::SendWait("^b")
Start-Sleep -Milliseconds 500

# 3. Inspector Info Panel (Ctrl+Shift+I)
Write-Host "Capturing 04: Inspector Info Panel..."
[System.Windows.Forms.SendKeys]::SendWait("^+i")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f04_inspector_info.png")

# 4. Inspector Files Panel (Ctrl+Shift+E)
Write-Host "Capturing 05: Inspector Files Panel..."
[System.Windows.Forms.SendKeys]::SendWait("^+e")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f05_inspector_files.png")

# 5. Inspector Git Panel (Ctrl+Shift+G)
Write-Host "Capturing 06: Inspector Git Panel..."
[System.Windows.Forms.SendKeys]::SendWait("^+g")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f06_inspector_git.png")

# Close inspector (Ctrl+Shift+I)
[System.Windows.Forms.SendKeys]::SendWait("^+i")
Start-Sleep -Milliseconds 400

# 6. Split Right (Ctrl+Shift+D)
Write-Host "Capturing 07: Split Right..."
[System.Windows.Forms.SendKeys]::SendWait("^+d")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "rust_f07_split_horizontal.png")

# 7. Split Down (Ctrl+Shift+R)
Write-Host "Capturing 08: Split Down Grid..."
[System.Windows.Forms.SendKeys]::SendWait("^+r")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "rust_f08_split_grid.png")

# 8. Pane Zoom (Ctrl+Shift+Z)
Write-Host "Capturing Pane Zoom..."
[System.Windows.Forms.SendKeys]::SendWait("^+z")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f09_pane_zoom.png")

# Restore zoom (Ctrl+Shift+Z)
[System.Windows.Forms.SendKeys]::SendWait("^+z")
Start-Sleep -Milliseconds 500

# 9. New Actions Dropdown Menu
Write-Host "Capturing New Menu Dropdown..."
$rect = New-Object Win32Audit+RECT
[Win32Audit]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
# Click the ▾ next to + in sidebar top
[Win32Audit]::ClickAt($rect.Left + 242, $rect.Top + 45)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f10_new_menu_dropdown.png")

# Dismiss dropdown with Esc
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 10. New Agent Dialog (F4)
Write-Host "Capturing New Agent Dialog..."
[System.Windows.Forms.SendKeys]::SendWait("{F4}")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f11_new_agent_dialog.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 11. Add Process Dialog (F6)
Write-Host "Capturing Add Process Dialog..."
[System.Windows.Forms.SendKeys]::SendWait("{F6}")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f12_add_process_dialog.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 12. Command Palette (Ctrl+Shift+P)
Write-Host "Capturing Command Palette..."
[System.Windows.Forms.SendKeys]::SendWait("^+p")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f13_command_palette.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 13. Shortcuts Overlay (F1)
Write-Host "Capturing Shortcuts Overlay..."
[System.Windows.Forms.SendKeys]::SendWait("{F1}")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f14_shortcuts_overlay.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 14. Settings Modal (Ctrl+,)
Write-Host "Capturing Settings Modal..."
[System.Windows.Forms.SendKeys]::SendWait("^,")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f15_settings_modal.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 15. Editor with Syntax Highlighting (F8 to open src/main.rs)
Write-Host "Capturing Editor Syntax Highlighting..."
[System.Windows.Forms.SendKeys]::SendWait("{F8}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "rust_f16_editor_syntax.png")

# 16. Editor with Markdown Live Preview
Write-Host "Capturing Editor Markdown Preview..."
[System.Windows.Forms.SendKeys]::SendWait("^p")
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("README{ENTER}")
Start-Sleep -Milliseconds 600
# Click Preview button in editor toolbar
[Win32Audit]::ClickAt($rect.Right - 80, $rect.Top + 62)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_f17_editor_markdown_preview.png")

# 17. Diff Split View (F7)
Write-Host "Capturing Diff Split View..."
# Modify buffer first via F10
[System.Windows.Forms.SendKeys]::SendWait("{F10}")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("{F7}")
Start-Sleep -Milliseconds 600
Capture-Window $hwnd (Join-Path $outDir "rust_f18_diff_split_view.png")

# 18. Diff with Git Staging Actions
Write-Host "Capturing Diff Git Staging..."
Capture-Window $hwnd (Join-Path $outDir "rust_f19_diff_git_staging.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

Write-Host "Stopping process..."
Stop-Process -Id $p.Id -Force

Write-Host "Generating side-by-side comparison images with original Go..."

# 1. Main window
Make-SideBySide `
    (Join-Path $origDir "01_omaterm_main_window.png") `
    (Join-Path $outDir "rust_f01_main_window.png") `
    (Join-Path $outDir "side_by_side_01_main_window.png") `
    "Main Application Shell"

# 2. Inspector Info
Make-SideBySide `
    (Join-Path $origDir "09_omaterm_inspector_info_panel.png") `
    (Join-Path $outDir "rust_f04_inspector_info.png") `
    (Join-Path $outDir "side_by_side_02_inspector_info.png") `
    "Inspector Info Panel"

# 3. Inspector Files
Make-SideBySide `
    (Join-Path $origDir "11_omaterm_inspector_files.png") `
    (Join-Path $outDir "rust_f05_inspector_files.png") `
    (Join-Path $outDir "side_by_side_03_inspector_files.png") `
    "Inspector Files Panel"

# 4. Inspector Git
Make-SideBySide `
    (Join-Path $origDir "12_omaterm_inspector_git.png") `
    (Join-Path $outDir "rust_f06_inspector_git.png") `
    (Join-Path $outDir "side_by_side_04_inspector_git.png") `
    "Inspector Git Panel"

# 5. Split Horizontal
Make-SideBySide `
    (Join-Path $origDir "10_omaterm_split_horizontal.png") `
    (Join-Path $outDir "rust_f07_split_horizontal.png") `
    (Join-Path $outDir "side_by_side_05_split_horizontal.png") `
    "Terminal Horizontal Split"

# 6. Multi Split Grid
Make-SideBySide `
    (Join-Path $origDir "14_omaterm_multi_split_workspace.png") `
    (Join-Path $outDir "rust_f08_split_grid.png") `
    (Join-Path $outDir "side_by_side_06_split_grid.png") `
    "Multi-Split Terminal Grid"

# 7. New Actions Menu
Make-SideBySide `
    (Join-Path $origDir "02_omaterm_new_menu_dropdown.png") `
    (Join-Path $outDir "rust_f10_new_menu_dropdown.png") `
    (Join-Path $outDir "side_by_side_07_new_menu_dropdown.png") `
    "New Actions Dropdown"

# 8. New Agent Dialog
Make-SideBySide `
    (Join-Path $origDir "05_omaterm_new_agent_dialog.png") `
    (Join-Path $outDir "rust_f11_new_agent_dialog.png") `
    (Join-Path $outDir "side_by_side_08_new_agent_dialog.png") `
    "New Agent Dialog"

# 9. Add Process Dialog
Make-SideBySide `
    (Join-Path $origDir "06_omaterm_add_process_dialog.png") `
    (Join-Path $outDir "rust_f12_add_process_dialog.png") `
    (Join-Path $outDir "side_by_side_09_add_process_dialog.png") `
    "Add Process Dialog"

# 10. Command Palette
Make-SideBySide `
    (Join-Path $origDir "03_omaterm_command_palette.png") `
    (Join-Path $outDir "rust_f13_command_palette.png") `
    (Join-Path $outDir "side_by_side_10_command_palette.png") `
    "Command Palette"

# 11. Shortcuts Overlay
Make-SideBySide `
    (Join-Path $origDir "08_omaterm_shortcuts_real.png") `
    (Join-Path $outDir "rust_f14_shortcuts_overlay.png") `
    (Join-Path $outDir "side_by_side_11_shortcuts_overlay.png") `
    "Shortcuts Overlay"

# 12. Settings Modal
Make-SideBySide `
    (Join-Path $origDir "07_omaterm_settings_real.png") `
    (Join-Path $outDir "rust_f15_settings_modal.png") `
    (Join-Path $outDir "side_by_side_12_settings_modal.png") `
    "Settings Modal"

Write-Host "Group F parity audit capture completed successfully!"
