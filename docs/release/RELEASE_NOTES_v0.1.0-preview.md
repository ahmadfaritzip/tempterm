# temp-terminal v0.1.0-preview (Windows x64 Release Notes)

Welcome to **temp-terminal v0.1.0-preview**, the complete native Rust rewrite of the `omaterm` agentic terminal workspace and process orchestrator.

## Overview
temp-terminal provides a headless daemon-first terminal engine with a fast, decoupled, GPU/Glow-accelerated graphical workspace (`egui`/`eframe`), dynamic split-pane tiling, full ConPTY Windows pseudoconsole integration, Git worktree isolation, agent process supervision, and persistent configuration.

## What's Included in the Bundle
- `temp-terminal-gui.exe` (~6.8 MB): Native desktop application featuring split-terminal workspace, command palette (`Ctrl+Shift+P`), file picker (`Ctrl+P`), embedded code editor, settings modal, and inspector.
- `temp-terminal-daemon.exe` (~2.0 MB): Background service for remote process execution, supervision, mailbox messaging, and devurl proxying.
- `temp-terminal.exe` (~1.3 MB): CLI client to manage daemon lifecycles, ping status, mailboxes, and supervised processes.
- `README.txt`: Quick-start and usage guide.

## System Requirements
- **OS:** Windows 10 (1809+) or Windows 11 (64-bit).
- **Architecture:** x86_64 (Intel/AMD 64-bit).
- **Graphics:** OpenGL 2.0+ compatible GPU or software rasterizer.

## Installation & First Run
1. Download and extract `temp-terminal-windows-x64.zip` to your desired folder (e.g. `C:\Tools\temp-terminal\`).
2. Double click `temp-terminal-gui.exe` to launch the workspace.
3. (Optional) Add the folder to your user `PATH` environment variable if you want to use the CLI `temp-terminal.exe` from any terminal.

## Key Features & Shortcuts
- `Ctrl+T`: New Terminal Session.
- `Ctrl+Shift+R`: Split Pane Right (vertical split).
- `Ctrl+Shift+D`: Split Pane Down (horizontal split).
- `Ctrl+W`: Close focused terminal pane.
- `Ctrl+P`: Quick Open file picker.
- `Ctrl+Shift+P`: Global Command Palette.
- `Ctrl+,`: Settings & Preferences (theme, font size, cursor blink, etc.).
- `F1` / `Ctrl+Shift+/`: Keyboard Shortcuts Cheatsheet.
- `Ctrl+B`: Toggle left projects/sessions sidebar.
- `Ctrl+Shift+B`: Toggle right inspector panel.

## Security & Network Configuration
- **Loopback Default:** By default, all daemon and IPC sockets bind strictly to `127.0.0.1` (loopback).
- **Remote Access Warning:** Explicitly binding non-loopback addresses (e.g. `0.0.0.0`) exposes the RPC control socket without authentication. Keep bindings on loopback unless running in a secure, isolated container or VPN.
- **Unsigned Binaries Notice:** This preview release is unsigned (no commercial Authenticode certificate attached). Windows SmartScreen may show an "Unrecognized app" prompt upon first launch; click **More info** -> **Run anyway**.
- **Process Isolation:** Terminal child processes are tracked using Windows Job Objects and automatically cleaned up upon pane or application exit.

## Checksums (SHA-256)
```
CA33031AAFBA5594E43A84EF753AFA78D301FE372C95EBE046898C86B85E7A20  temp-terminal-windows-x64.zip
F5BFEC617964A2894C371F2B0A02022CCF8FE6D7E5BBB24EC7124F5FF009228E  temp-terminal-gui.exe
827DBB3CD91F012D5F7A58D401D423DEFEC5E60B6ED261EACC1FC237B16FF1F0  temp-terminal-daemon.exe
FF3D667DAA500ABF3E9A665A2F64E5989E251DFCE6CB1118126962BD3D87F632  temp-terminal.exe
```
