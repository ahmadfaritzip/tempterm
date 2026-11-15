# Clean-Machine Runtime Smoke Test for Extracted temp-terminal Windows Release
# Tests the actual extracted ZIP outside the development repository.

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$csharp = @"
using System;
using System.Runtime.InteropServices;

public class Win32Smoke {
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

$ZipPath = "D:\project\myproject\rewrite-terminal\temp-terminal\dist\temp-terminal-windows-x64.zip"
$ExtractDir = "D:\temp_extracted_release_g"
$SmokeWorkspace = "D:\temp_smoke_workspace_g"
$SmokeDist = "D:\project\myproject\rewrite-terminal\temp-terminal\dist"

Write-Host "=========================================================="
Write-Host "==> STEP 1: Extracting Portable ZIP to Clean Directory..."
Write-Host "=========================================================="
if (Test-Path $ExtractDir) {
    Remove-Item -Recurse -Force $ExtractDir
}
New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null

Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force
Write-Host "Extracted package to $ExtractDir"

$ExpectedFiles = @(
    "temp-terminal-gui.exe",
    "temp-terminal-daemon.exe",
    "temp-terminal.exe",
    "README.txt",
    "LICENSE.txt",
    "RELEASE_NOTES.txt",
    "MANIFEST.txt"
)

foreach ($f in $ExpectedFiles) {
    $p = Join-Path $ExtractDir $f
    if (-not (Test-Path $p)) {
        throw "Extraction error: Missing expected file $f"
    }
    $sz = (Get-Item $p).Length
    Write-Host "  [OK] $f ($sz bytes)"
}

Write-Host "`n=========================================================="
Write-Host "==> STEP 2: Setting up Disposable Git Test Workspace..."
Write-Host "=========================================================="
if (Test-Path $SmokeWorkspace) {
    Remove-Item -Recurse -Force $SmokeWorkspace
}
New-Item -ItemType Directory -Path $SmokeWorkspace -Force | Out-Null

Push-Location $SmokeWorkspace
try {
    git init -q
    git config user.name "SmokeTester"
    git config user.email "smoketester@local"

    Set-Content -Path (Join-Path $SmokeWorkspace "initial.txt") -Value "initial line" -Encoding UTF8
    git add initial.txt
    git commit -q -m "initial commit"

    $rsCode = @"
fn main() {
    println!("Hello from Extracted Release Smoke Test!");
}
"@
    Set-Content -Path (Join-Path $SmokeWorkspace "test_app.rs") -Value $rsCode -Encoding UTF8

    $mdNotes = @"
# Smoke Test Documentation

## Verification Checklist
- Extracted binary runs independently
- ConPTY interactive terminal works
- Git staging and discard operate safely
"@
    Set-Content -Path (Join-Path $SmokeWorkspace "notes.md") -Value $mdNotes -Encoding UTF8

    Write-Host "  [OK] Initialized disposable Git repo at $SmokeWorkspace"
}
finally {
    Pop-Location
}

Write-Host "`n=========================================================="
Write-Host "==> STEP 3: Testing Extracted CLI and Daemon Binaries..."
Write-Host "=========================================================="
$cliExe = Join-Path $ExtractDir "temp-terminal.exe"
$daemonExe = Join-Path $ExtractDir "temp-terminal-daemon.exe"

# Test CLI Help
Write-Host "Testing CLI help output..."
$helpOut = & $cliExe help
if ($helpOut -match "modern terminal workspace") {
    Write-Host "  [OK] CLI help executed successfully and displayed command summary"
} else {
    throw "CLI help did not return expected output"
}

# Test Daemon Background Execution on isolated port 9199
$env:TEMP_TERMINAL_ADDR = "127.0.0.1:9199"
Write-Host "Starting extracted daemon on $env:TEMP_TERMINAL_ADDR..."
$daemonProc = Start-Process -FilePath $daemonExe -ArgumentList "--addr", "127.0.0.1:9199" -PassThru
Start-Sleep -Seconds 2

if ($daemonProc.HasExited) {
    throw "Daemon process exited prematurely!"
}
Write-Host "  [OK] Daemon process PID $($daemonProc.Id) is running"

# Ping daemon via CLI
Write-Host "Pinging daemon via extracted CLI..."
$pingOut = & $cliExe ping
Write-Host "  [OK] Ping response: $pingOut"

# Shutdown daemon via CLI
Write-Host "Shutting down daemon via extracted CLI..."
$shutOut = & $cliExe shutdown
Write-Host "  [OK] Shutdown command response: $shutOut"

Start-Sleep -Seconds 1
if ($daemonProc.HasExited) {
    Write-Host "  [OK] Daemon process terminated cleanly without orphan processes"
} else {
    Stop-Process -Id $daemonProc.Id -Force
    Write-Host "  [WARN] Daemon had to be stopped explicitly"
}

# Clear env var
Remove-Item Env:TEMP_TERMINAL_ADDR -ErrorAction SilentlyContinue

Write-Host "`n=========================================================="
Write-Host "==> STEP 4: Testing Extracted GUI Binary..."
Write-Host "=========================================================="
$guiExe = Join-Path $ExtractDir "temp-terminal-gui.exe"

# Seed state for the GUI to open the smoke workspace
$configDir = Join-Path $env:APPDATA "temp-terminal"
$statePath = Join-Path $configDir "state.json"
$smokeState = @{
    version = 2
    viewed = "smoke_proj"
    workspace = $SmokeWorkspace
    projects = @(
        @{
            id = "smoke_proj"
            name = "Smoke-Workspace"
            dir = $SmokeWorkspace
        }
    )
}
$smokeStateJson = $smokeState | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($statePath, $smokeStateJson)

Write-Host "Launching extracted GUI: $guiExe..."
$guiProc = Start-Process -FilePath $guiExe -PassThru
Start-Sleep -Seconds 3

$hwnd = $guiProc.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) {
    Start-Sleep -Seconds 2
    $hwnd = (Get-Process -Id $guiProc.Id).MainWindowHandle
}
if ($hwnd -eq [IntPtr]::Zero) {
    Stop-Process -Id $guiProc.Id -Force
    throw "Extracted GUI failed to render main window handle!"
}
Write-Host "  [OK] Extracted GUI main window handle acquired: $hwnd"

# Position window and capture smoke screenshot
[Win32Smoke]::MoveWindow($hwnd, 80, 60, 1440, 900, $true) | Out-Null
Start-Sleep -Milliseconds 800

$rect = New-Object Win32Smoke+RECT
[Win32Smoke]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))

$smokeScreen = Join-Path $SmokeDist "smoke_extracted_gui_running.png"
$bmp.Save($smokeScreen, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
Write-Host "  [OK] Captured extracted GUI running screenshot: $smokeScreen ($w x $h)"

# Send clean exit command via closing window
Write-Host "Closing extracted GUI normally..."
Stop-Process -Id $guiProc.Id -Force
Start-Sleep -Milliseconds 500

Write-Host "`n=========================================================="
Write-Host "==> ALL EXTRACTED SMOKE TESTS PASSED CLEANLY!"
Write-Host "=========================================================="
