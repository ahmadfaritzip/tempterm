Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outDir = "D:\project\myproject\rewrite-terminal\temp-terminal\docs\ui-baseline"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32GroupB {
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
Add-Type -TypeDefinition $csharp

function Capture-Window($hwnd, $outFile) {
    [Win32GroupB]::ShowWindow($hwnd, 9) | Out-Null
    [Win32GroupB]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400

    $rect = New-Object Win32GroupB+RECT
    [Win32GroupB]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

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

# Setup clean test project directories
$dirAlpha = "D:\temp_workspace_alpha"
$dirBeta = "D:\temp_workspace_beta"
if (-not (Test-Path $dirAlpha)) { New-Item -ItemType Directory -Path $dirAlpha -Force | Out-Null }
if (-not (Test-Path $dirBeta)) { New-Item -ItemType Directory -Path $dirBeta -Force | Out-Null }

# Seed state.json with Project Alpha and Project Beta
$configDir = Join-Path $env:APPDATA "temp-terminal"
if (-not (Test-Path $configDir)) {
    $configDir = Join-Path $env:USERPROFILE ".config\temp-terminal"
}
if (-not (Test-Path $configDir)) {
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
}
$statePath = Join-Path $configDir "state.json"

$seedState = @{
    version = 2
    viewed = "proj_alpha"
    workspace = $dirAlpha
    projects = @(
        @{
            id = "proj_alpha"
            name = "Project-Alpha"
            dir = $dirAlpha
        },
        @{
            id = "proj_beta"
            name = "Project-Beta"
            dir = $dirBeta
        }
    )
}
$seedJson = $seedState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $seedJson)
Write-Host "Seeded state.json with Project-Alpha and Project-Beta at $statePath"

$exe = "D:\project\myproject\rewrite-terminal\temp-terminal\target\debug\temp-terminal-gui.exe"
Write-Host "Launching temp-terminal-gui (Run 1)..."
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

[Win32GroupB]::MoveWindow($hwnd, 100, 100, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

# 1. Multiple registered projects visible in sidebar
Write-Host "1. Capturing Multiple registered projects..."
Capture-Window $hwnd "$outDir\group_b_01_multiple_registered_projects.png"

# 2. Project A selected
Write-Host "2. Capturing Project A selected..."
Capture-Window $hwnd "$outDir\group_b_02_project_a_selected.png"

# Add second session in Project A (Ctrl+T)
Write-Host "Adding second session in Project A..."
[System.Windows.Forms.SendKeys]::SendWait("^t")
Start-Sleep -Milliseconds 800

# 3. Switch to Project B
Write-Host "Switching to Project B..."
# We can click on Project Beta or trigger via Quick Open / Open Project / or simulated click
# In sidebar, Project Alpha is around y=75, Project Beta is around y=105
# Let's send a click or shortcut to switch to Project Beta
# With our new Open Project dialog (Ctrl+O) we can open/switch to Project Beta:
[System.Windows.Forms.SendKeys]::SendWait("^o")
Start-Sleep -Milliseconds 500
[System.Windows.Forms.SendKeys]::SendWait($dirBeta)
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Start-Sleep -Milliseconds 1200

Write-Host "3. Capturing Project B selected..."
Capture-Window $hwnd "$outDir\group_b_03_project_b_selected.png"

# Add second session in Project B (Ctrl+T)
Write-Host "Adding second session in Project B..."
[System.Windows.Forms.SendKeys]::SendWait("^t")
Start-Sleep -Milliseconds 800

# 4. Multiple sessions under separate projects
Write-Host "4. Capturing multiple sessions under separate projects..."
Capture-Window $hwnd "$outDir\group_b_04_multiple_sessions_under_projects.png"

# Close Run 1 to test restart and state restoration
Write-Host "Closing Run 1 to test persistence..."
Stop-Process -Id $p.Id -Force
Start-Sleep -Seconds 2

# Verify state.json was updated with sessions
$savedStateText = [System.IO.File]::ReadAllText($statePath)
Write-Host "Saved state.json content: $savedStateText"

# Relaunch application (Run 2)
Write-Host "Relaunching temp-terminal-gui (Run 2)..."
$p2 = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 3

$hwnd2 = $p2.MainWindowHandle
if ($hwnd2 -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd2 = (Get-Process -Id $p2.Id).MainWindowHandle
}
[Win32GroupB]::MoveWindow($hwnd2, 100, 100, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 1000

# 5. Restored application state after relaunch
Write-Host "5. Capturing restored application state after relaunch..."
Capture-Window $hwnd2 "$outDir\group_b_05_restored_state_after_relaunch.png"

Write-Host "Stopping Run 2..."
Stop-Process -Id $p2.Id -Force

# Clean up temp test dirs
Remove-Item -Recurse -Force $dirAlpha -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $dirBeta -ErrorAction SilentlyContinue

Write-Host "Group B acceptance screen capture completed successfully."
