Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline\group-g"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32GroupG {
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
    [Win32GroupG]::ShowWindow($hwnd, 9) | Out-Null
    [Win32GroupG]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32GroupG+RECT
    [Win32GroupG]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

Write-Host "Setting up multi-project test workspace for Group G..."
$dirTestA = "D:\temp_workspace_g_a"
$dirTestB = "D:\temp_workspace_g_b"
if (-not (Test-Path $dirTestA)) { New-Item -ItemType Directory -Path $dirTestA -Force | Out-Null }
if (-not (Test-Path $dirTestB)) { New-Item -ItemType Directory -Path $dirTestB -Force | Out-Null }

$srcDirA = Join-Path $dirTestA "src"
if (-not (Test-Path $srcDirA)) { New-Item -ItemType Directory -Path $srcDirA -Force | Out-Null }

$mainRs = @"
// temp-terminal Project A
fn main() {
    println!("Hello from Project A");
}
"@
[System.IO.File]::WriteAllText((Join-Path $srcDirA "main.rs"), $mainRs)

$readmeMd = @"
# Project A Documentation

A verified release candidate workspace.

## Features
- Full Go-to-Rust feature parity
- Markdown live preview
- Safe Git staging and discard with confirmation
"@
[System.IO.File]::WriteAllText((Join-Path $dirTestA "README.md"), $readmeMd)

# Seed state.json with 2 projects
$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
$statePath = Join-Path $configDir "state.json"
$settingsPath = Join-Path $configDir "settings.json"

$seedState = @{
    version = 2
    viewed = "proj_a"
    workspace = $dirTestA
    projects = @(
        @{
            id = "proj_a"
            name = "Project-Alpha"
            dir = $dirTestA
        },
        @{
            id = "proj_b"
            name = "Project-Beta"
            dir = $dirTestB
        }
    )
}
$seedJson = $seedState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $seedJson)

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

[Win32GroupG]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Multi-project switching sidebar view
Write-Host "Capturing G01: Multi-Project Sidebar View..."
Capture-Window $hwnd (Join-Path $outDir "rust_g01_multi_project_sidebar.png")

# 2. Open Settings (Ctrl+,) and capture Terminal settings tab
Write-Host "Capturing G02: Settings Terminal Tab..."
[System.Windows.Forms.SendKeys]::SendWait("^,")
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_g02_settings_terminal_tab.png")

# Click Editor tab in Settings modal (second item on left list)
$rect = New-Object Win32GroupG+RECT
[Win32GroupG]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
[Win32GroupG]::ClickAt($rect.Left + 350, $rect.Top + 245)
Start-Sleep -Milliseconds 400
Write-Host "Capturing G03: Settings Editor Tab..."
Capture-Window $hwnd (Join-Path $outDir "rust_g03_settings_editor_tab.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

# 3. Open README.md and toggle Markdown Preview
Write-Host "Capturing G04: Markdown Live Preview..."
[System.Windows.Forms.SendKeys]::SendWait("^p")
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("README{ENTER}")
Start-Sleep -Milliseconds 600
# Click Preview button in editor toolbar
[Win32GroupG]::ClickAt($rect.Right - 80, $rect.Top + 62)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_g04_markdown_preview_rich.png")

# 4. Open Diff and click Discard to trigger confirmation modal/prompt
Write-Host "Capturing G05: Diff Discard Confirmation Prompt..."
# Make buffer dirty with F10
[System.Windows.Forms.SendKeys]::SendWait("{F10}")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("{F7}")
Start-Sleep -Milliseconds 600
# Click Discard in toolbar (near top right)
[Win32GroupG]::ClickAt($rect.Right - 170, $rect.Top + 62)
Start-Sleep -Milliseconds 500
Capture-Window $hwnd (Join-Path $outDir "rust_g05_diff_discard_confirm_prompt.png")
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

Write-Host "Stopping process..."
Stop-Process -Id $p.Id -Force
Write-Host "Group G evidence capture complete!"
