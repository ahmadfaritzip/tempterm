# Packaging Script for temp-terminal Windows Release
# Builds release binaries, structures the distribution directory, and generates a standalone ZIP.

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = (Resolve-Path "$ScriptDir\..").Path
$DistDir = Join-Path $WorkspaceRoot "dist"
$PackageDir = Join-Path $DistDir "temp-terminal-windows-x64"

Write-Host "==> Building temp-terminal workspace in release mode..."
Push-Location $WorkspaceRoot
try {
    cargo build --release --workspace --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo release build failed with exit code $LASTEXITCODE"
    }

    Write-Host "==> Preparing distribution folder: $PackageDir"
    if (Test-Path $PackageDir) {
        Remove-Item -Recurse -Force $PackageDir
    }
    New-Item -ItemType Directory -Path $PackageDir -Force | Out-Null

    # Copy binary executables
    $Binaries = @("temp-terminal.exe", "temp-terminal-gui.exe", "temp-terminal-daemon.exe")
    foreach ($bin in $Binaries) {
        $src = Join-Path $WorkspaceRoot "target\release\$bin"
        if (-not (Test-Path $src)) {
            throw "Expected binary not found: $src"
        }
        Copy-Item -Path $src -Destination $PackageDir -Force
        Write-Host "    Copied $bin"
    }

    # Copy license
    if (Test-Path "$WorkspaceRoot\LICENSE") {
        Copy-Item "$WorkspaceRoot\LICENSE" -Destination (Join-Path $PackageDir "LICENSE.txt") -Force
        Write-Host "    Copied LICENSE.txt"
    }

    # Create README for the release bundle
    $ReadmeContent = @'
# temp-terminal (Windows x64 Release v0.1.0-preview)

temp-terminal is a native Windows terminal workspace, in-memory editor, and process supervisor written in pure Rust.

## Included Executables
- `temp-terminal-gui.exe`: Graphical Windows desktop terminal workspace application (egui/eframe).
- `temp-terminal-daemon.exe`: Headless background session, process, and mailbox supervisor daemon.
- `temp-terminal.exe`: Command-line tool (CLI) for controlling processes and agent mailboxes.

## Quick Start
1. Run `temp-terminal-gui.exe` to launch the graphical terminal workspace.
2. Run `temp-terminal.exe help` in your terminal to see CLI commands.
3. Run `temp-terminal-daemon.exe` if running in headless server mode.

## Security & Runtime Notices
- Unsigned Distribution: This build is unsigned. Windows SmartScreen may show an "Unrecognized app" prompt on initial launch; select "More info" -> "Run anyway".
- Loopback Binding: By default, the daemon binds strictly to 127.0.0.1:9099. Non-loopback binding requires explicit opt-in.
- Destructive Operations: Git Discard operations require explicit confirmation in the UI to prevent data loss.

## Environment Variables & Configuration
- `TEMP_TERMINAL_ADDR`: Daemon TCP socket address (default: 127.0.0.1:9099).
- `TEMP_TERMINAL_CONFIG_HOME`: Custom configuration root (defaults to %APPDATA%\temp-terminal).
- `TEMP_TERMINAL_DEV`: Set to 1 for development mode (uses local state).

## Clean Shutdown
- GUI: Press Ctrl+W to close active pane, or close the window.
- Daemon: Send Ctrl+C or run `temp-terminal.exe shutdown`.

For documentation and project details:
https://github.com/fachrihawari/omaterm
'@
    Set-Content -Path (Join-Path $PackageDir "README.txt") -Value $ReadmeContent -Encoding UTF8

    # Create Release Notes
    $ReleaseNotesContent = @'
# temp-terminal Release Notes - v0.1.0-preview (Windows x64)

## Highlights
- **100% Pure Rust Architecture**: Migrated from Go (~73k LoC) to safe, headless Rust (Rust 1.83, MSRV pinned).
- **GPU-Accelerated Workspace**: Real egui/eframe desktop UI running at smooth 60+ FPS with Glow rendering.
- **ConPTY Interactive Terminal**: Full Windows ConPTY terminal engine with VT/ANSI processing, UTF-8 chunk buffering, PSReadLine arrow-navigation fix, and auto-copy text selection.
- **Multi-Split Panes & Layouts**: Binary tree pane hierarchy with drag divider resizing, 120px safety clamping, and Ctrl+Shift+Z pane zoom.
- **In-Memory Editor & Syntax Highlighting**: Fast in-memory editor supporting 15+ programming languages, 32MB file size guard, binary data guard, external modification conflict detection, and atomic saving.
- **Markdown Live Preview**: Centered 720 DIP reading view with headers, code blocks, lists, blockquotes, and instant Preview/Source toggle.
- **Git Staging & Diff Viewer**: Side-by-side split diff, inline unified diff, with Stage File, Unstage File, and confirmed Discard actions.
- **Process Supervision**: Headless supervisor daemon with auto-restart backoff, port detection, and atomic YAML preservation.
'@
    Set-Content -Path (Join-Path $PackageDir "RELEASE_NOTES.txt") -Value $ReleaseNotesContent -Encoding UTF8

    # Create Release Manifest
    $ManifestContent = @'
Product: temp-terminal
Version: 0.1.0-preview
Target Architecture: Windows x64 (x86_64-pc-windows-msvc)
Rust Toolchain: 1.83.0 (Edition 2021)
Build Profile: Release (optimized)
Included Executables:
  - temp-terminal-gui.exe (Desktop Workspace UI)
  - temp-terminal-daemon.exe (Headless Supervisor Daemon)
  - temp-terminal.exe (CLI Management Client)
Signing Status: Unsigned (Developer Preview)
Runtime Dependencies: Windows 10/11 x64, PowerShell (powershell.exe / pwsh.exe)
'@
    Set-Content -Path (Join-Path $PackageDir "MANIFEST.txt") -Value $ManifestContent -Encoding UTF8

    # Create ZIP archive
    $ZipPath = Join-Path $DistDir "temp-terminal-windows-x64.zip"
    if (Test-Path $ZipPath) {
        Remove-Item -Force $ZipPath
    }
    Write-Host "==> Archiving release bundle to $ZipPath..."
    Compress-Archive -Path "$PackageDir\*" -DestinationPath $ZipPath -Force

    # Generate SHA-256 Checksums
    $ChecksumFile = Join-Path $DistDir "SHA256SUMS.txt"
    $lines = @()
    $zipHash = (Get-FileHash -Algorithm SHA256 -Path $ZipPath).Hash
    $lines += "$zipHash  temp-terminal-windows-x64.zip"

    foreach ($bin in $Binaries) {
        $p = Join-Path $PackageDir $bin
        $h = (Get-FileHash -Algorithm SHA256 -Path $p).Hash
        $lines += "$h  $bin"
    }
    $readmeHash = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $PackageDir "README.txt")).Hash
    $lines += "$readmeHash  README.txt"

    Set-Content -Path $ChecksumFile -Value ($lines -join "`r`n") -Encoding UTF8

    Write-Host "==> Release package successfully generated at: $ZipPath"
    Write-Host "==> Checksums written to: $ChecksumFile"
}
finally {
    Pop-Location
}
