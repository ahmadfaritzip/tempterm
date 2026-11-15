# Technical Report: Integrated PowerShell & Keyboard Input Routing Fix

## 1. Overview & Context

This critical fix resolves two usability-blocking defects in `temp-terminal v0.1.0-preview`:
1. The integrated workspace previously defaulted to `cmd.exe` instead of native PowerShell.
2. Keyboard input was failing to register in the packaged GUI due to missing egui keyboard focus and event handling mismatches.

Both issues have been resolved, regression tested, packaged, and verified on the actual Windows desktop.

---

## 2. Root Cause Analysis

### 2.1 CMD Startup Root Cause
- `trigger_new_session` in `workspace.rs` and `split_active_pane` in `lib.rs` hardcoded `cmd.exe` with arguments `["cmd.exe", "cmd", "/d"]`.
- There was no platform shell discovery mechanism to locate `pwsh.exe` or `powershell.exe`.

### 2.2 Broken Keyboard Input Root Cause
- In egui, widgets do not receive key and text events unless they actively hold keyboard focus. The terminal viewport used `ui.interact(terminal_area, id, Sense::click_and_drag())` but never requested keyboard focus in egui's memory subsystem.
- Clicking a pane called `focus_pane(pane_id)` which spawned an async tokio task to update `AppHost`, leaving `self.snapshot.active_pane_id` stale until the subsequent frame, causing dropped or misrouted events.
- Text events and key events were processed without character filtering, risking duplication of keystrokes.
- Navigation keys (`Home`, `End`, `Delete`, `PageUp`, `PageDown`) lacked ANSI escape code sequences in the key handler.

---

## 3. Implementation Details

### 3.1 Deterministic PowerShell Resolution (`temp_terminal_ui::resolve_default_shell`)
- Searches system `PATH` and `%ProgramFiles%\PowerShell\7\pwsh.exe` for PowerShell 7 (`pwsh.exe`).
- Falls back to `powershell.exe` on system `PATH` and `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`.
- Supplies `-NoLogo` flag to ensure a clean interactive prompt.
- Refuses silent fallback to `cmd.exe`. If neither is found, returns an explicit `UiError::ShellNotFound`.
- Integrated across `trigger_new_session` and `split_active_pane`. User-defined commands in New Agent and Add Process are preserved untouched.

### 3.2 Keyboard Event Routing & Focus Retention
- **Egui Focus Request:** Active pane checks and requests focus on every frame via `ui.memory_mut(|m| m.request_focus(term_id_salt))`.
- **Immediate Local Focus Update:** `focus_pane` updates `self.snapshot.active_pane_id` and pane active flags immediately, eliminating async lag.
- **Deduplication:** Committed characters arrive via `egui::Event::Text(t)` (filtered for non-control characters). Non-printable keys (Enter, Backspace, Tab, Arrows, Home, End, Delete, PageUp, PageDown) are handled via `egui::Event::Key`.
- **Shortcuts:**
  - `Ctrl+C`: Sends `0x03` interrupt when no text is selected.
  - `Ctrl+V` / `Ctrl+Shift+V`: Pastes clipboard content with CRLF normalization.
  - `Ctrl+Shift+C`: Copies selected text.
  - `Ctrl+D`: Sends `0x04` EOF.
  - `Ctrl+L`: Sends `0x0c` form feed (clear).
  - `Ctrl+W`: Closes active pane and terminates process.
  - `Ctrl+Tab` / `Ctrl+Shift+Tab`: Cycles active pane focus.
  - `F2` / `F3`: Splits active pane vertically / horizontally.

---

## 4. Verification & Desktop Acceptance Results

### 4.1 Unit & Integration Tests
- `test_powershell_resolution_and_default_command`: Proves resolution finds PowerShell, excludes `cmd.exe`, and attaches `-NoLogo`.
- `test_workspace_pane_focus_switching_and_routing`: Proves split panes spawn with PowerShell, immediate focus switching between panes, and clean teardown.
- All 183 workspace unit and integration tests passed.

### 4.2 Windows Desktop Acceptance Test
Extracted from `temp-terminal/dist/temp-terminal-windows-x64.zip` and tested live on Windows:
1. `+ New Session` spawned `powershell.exe` with `PS D:\project\myproject\rewrite-terminal>`.
2. Typed `Write-Output "POWERSHELL_INPUT_OK"` and pressed `Enter`. Output `POWERSHELL_INPUT_OK` rendered visibly on screen (captured in `release_v010_07_powershell_input_executed.png`).
3. Pressed `F2`. Second pane spawned a second PowerShell instance (captured in `release_v010_08_two_powershell_panes.png`).
4. Typed `Write-Output "SECOND_PANE_P1_OK"` and pressed `Enter`. Output rendered visibly in the second pane without corrupting the first (captured in `release_v010_09_second_pane_executed.png`).
5. Closed GUI and daemon; all PowerShell processes terminated cleanly with zero orphans.

---

## 5. Artifacts & Checksums

| Artifact File | Size (Bytes) | SHA-256 Checksum |
|---|---|---|
| `temp-terminal-gui.exe` | 6,055,936 | `69B0FF293FBA7A9F62031795DCC65F2858909448FE737EA22E1B9DC9CCC9806D` |
| `temp-terminal-daemon.exe` | 2,003,456 | `827DBB3CD91F012D5F7A58D401D423DEFEC5E60B6ED261EACC1FC237B16FF1F0` |
| `temp-terminal.exe` | 1,261,056 | `FF3D667DAA500ABF3E9A665A2F64E5989E251DFCE6CB1118126962BD3D87F632` |
| `README.txt` | 1,191 | `149666F947CD8EE82DD7E2FF61DE6667A6C97FD4FD60878F7F8B1E5977CD3B3F` |
| **`temp-terminal-windows-x64.zip`** | **3,833,394** | **`B41542E19CE40716B7F537878268E64772F83D93D888DE8656CB19346C0D264E`** |

---

## 6. Git Checkpoint

- **Branch:** `feat/p1-02-core-proto`
- **Parent Commit:** `cd56b7025a7a50d6155693a5e1416361edb70edd`
- **Documentation:** Recorded in `FINAL_RELEASE_HANDOFF_REPORT.md` and `POWERSHELL_KEYBOARD_INPUT_FIX_REPORT.md`.
