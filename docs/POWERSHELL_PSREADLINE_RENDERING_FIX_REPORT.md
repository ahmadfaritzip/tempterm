# Technical Report: PowerShell PSReadLine Arrow Navigation Stale Text Rendering Fix

## 1. Executive Summary & Problem Statement

When using the integrated PowerShell terminal, users observed that recalling commands from history using **Arrow Up** and subsequently clearing or navigating away using **Arrow Down** left stale command suffixes on screen.

For example:
- **Arrow Up recalled:** `PS D:\...> ii .\temp-terminal\dist\temp-terminal-windows-x64\`
- **Arrow Down cleared:** `PS D:\...>                                  inal-windows-x64\`

The trailing suffix `inal-windows-x64\` remained visible as ghost characters.

This report documents the root-cause diagnostics, the exact VT sequence behavior captured from ConPTY and PSReadLine, the implementation in `temp-terminal-ui`, regression tests, real Windows desktop acceptance verification, and refreshed release package checksums.

---

## 2. VT Diagnostics & Root Cause Analysis

### 2.1 Trace Pipeline
`PowerShell 5.1 (PSReadLine 2.3.5)` → `ConPTY Pseudoconsole` → `temp-terminal-core::Registry` (Output Subscriber) → `TerminalBuffer` (`append_output`) → `TerminalRenderer` (`parse_ansi_line`) → `egui WorkspaceApp`.

### 2.2 Captured Raw VT Bytes from ConPTY
When typing a command, recalling history, and erasing or clearing the line, PSReadLine 2.3.5 and ConPTY emit the following sequences:

1. **Typing with History Prediction:**
   ```
   \x1b[?25l\x1b[93mii \x1b[37m.\\temp-terminal\\dist\\temp-terminal-windows-x64\\\x1b[4;39H\x1b[?25h
   ```
2. **Line Erasure / Backspace / Escape:**
   ```
   \x1b[m\x1b[?25l\x1b[4;38H\x1b[50X\x1b[?25h
   ```
   - `\x1b[4;38H`: Cursor Position to Row 4, Col 38 (the start of the command line after prompt).
   - `\x1b[50X`: **Erase Characters (ECH)** — erases 50 characters starting at the cursor position by replacing them with blank spaces.

### 2.3 The Root Causes

1. **Missing Support for `CSI X` (Erase Characters - ECH):**
   - The VT parser in `TerminalBuffer` previously handled `CSI K` (Erase in Line) and `CSI J` (Erase in Display), but completely ignored `CSI X`.
   - When PSReadLine erased a recalled command or inline prediction, it sent `\x1b[{N}X`.
   - Because `X` was unhandled, the 50 characters were never overwritten with spaces, leaving the old characters in the screen line buffer.
2. **Incomplete Chunk Boundary Splitting for Escape Sequences:**
   - Previously, `append_output` only buffered incomplete UTF-8 bytes (`utf8_remainder`).
   - If an escape sequence (`\x1b[4;38H` or `\x1b[50X`) was split across two PTY read chunks (e.g. `\x1b[` in chunk 1 and `50X` in chunk 2), the leading `\x1b[` was discarded or corrupted, causing the subsequent coordinates or command characters to be printed as literal text.
3. **Missing Extended Cursor Positioning Commands:**
   - PSReadLine also emits `CSI G` (Cursor Horizontal Absolute) and `CSI d` (Line Position Absolute), which were previously ignored.

---

## 3. Implementation Details

In `temp-terminal/crates/temp-terminal-ui/src/lib.rs`:

1. **Erase Characters (ECH `\x1b[{N}X`):**
   - Added handling for `CSI X`: parses character count $N$ (defaulting to 1).
   - If `cursor_col < line.chars().count()`, replaces $N$ characters starting at `cursor_col` with ASCII spaces (`" ".repeat(erase_count)`), properly preserving trailing text and cell alignment.
2. **Cursor Positioning Enhancements:**
   - Added `CSI G` (Cursor Horizontal Absolute): sets `cursor_col = (col - 1).min(cols - 1)`.
   - Added `CSI d` (Line Position Absolute): sets `cursor_row = (row - 1).min(rows - 1)`.
3. **Escape Sequence Chunk Boundary Buffering (`escape_remainder`):**
   - Added `escape_remainder: Vec<u8>` to `TerminalBuffer`.
   - Any incomplete CSI (`\x1b[...`) or OSC sequence reaching the end of an incoming PTY chunk is preserved in `escape_remainder` and prepended to the next chunk before execution.
4. **ANSI String Sanitization:**
   - In `CSI 0 K`, `truncate(byte_start)` correctly trims the line using character byte index boundaries rather than byte lengths, preventing multi-byte UTF-8 panics.

---

## 4. Regression Testing & Windows Acceptance Verification

### 4.1 Automated Unit & Integration Tests
Added in `temp-terminal/crates/temp-terminal-ui/tests/ui_frontend_integration_test.rs`:
- `test_terminal_buffer_ech_erase_characters_and_csi_k`: Verifies that a prompt followed by a long command, cursor repositioning, and `CSI X` / `CSI K` leaves every erased cell blank with no stale suffix.
- `test_terminal_buffer_csi_and_utf8_split_at_all_chunk_boundaries`: Splits ANSI escape sequences across 1-byte, 2-byte, and 3-byte chunk boundaries, proving state continuity without dropped sequences.
- `test_terminal_buffer_psreadline_escape_line_clearing`: Simulates exact PSReadLine escape sequence stream (`\x1b[1;19H\x1b[50X`), verifying that the line after the prompt contains only blank spaces.
- **Full Workspace Test Suite:** 186 unit/integration tests passed across all 9 crates (0 failures).

### 4.2 Windows Desktop Acceptance Test
Tested on the live Windows desktop using the release build of `temp-terminal-gui.exe`:
1. Started integrated PowerShell session.
2. Ran a long command: `ii .\temp-terminal\dist\temp-terminal-windows-x64\`.
3. Ran a short command: `dir`.
4. Pressed **Arrow Up once**: Recalled `dir` cleanly.
5. Pressed **Arrow Up twice**: Recalled the long command (`psreadline_fix_06_up_long.png`).
6. Pressed **Arrow Down once**: Returned to `dir` with **zero stale suffix** (`psreadline_fix_07_down_dir_clean.png`).
7. Pressed **Arrow Down again**: Returned to the empty prompt line with **zero ghost characters** (`psreadline_fix_08_down_empty_clean.png`).
8. Typed and executed subsequent commands (`Get-Date`), confirming that normal shell execution continues unimpeded.

---

## 5. Distributable Artifact Inventory & SHA-256 Checksums

The release archive was regenerated with `--release --workspace --locked` (Rust 1.83.0 MSRV):

| Artifact File | Size (Bytes) | SHA-256 Checksum |
|---|---|---|
| `temp-terminal-gui.exe` | 6,060,032 | `AA694FB53525F702B3FEFA3B4A06D76A615FAC3A37464AE06B1B82E79A58C786` |
| `temp-terminal-daemon.exe` | 2,003,456 | `827DBB3CD91F012D5F7A58D401D423DEFEC5E60B6ED261EACC1FC237B16FF1F0` |
| `temp-terminal.exe` | 1,261,056 | `FF3D667DAA500ABF3E9A665A2F64E5989E251DFCE6CB1118126962BD3D87F632` |
| `README.txt` | 1,191 | `149666F947CD8EE82DD7E2FF61DE6667A6C97FD4FD60878F7F8B1E5977CD3B3F` |
| **`temp-terminal-windows-x64.zip`** | **3,837,931** | **`6D86BF913A17237244CFE76C9F4C5B2DD5F63300A350C3F50466EF1D9753C741`** |

---

## 6. Git Provenance

- **Branch:** `feat/p1-02-core-proto`
- **Parent Commit:** `b84e798e356b536b55bd06fa6c97cb74f8a3ce91`
- **Commit Subject:** `fix(ui): psreadline erase characters (ECH) and arrow navigation rendering`
