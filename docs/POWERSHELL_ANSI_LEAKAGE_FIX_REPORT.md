# Technical Report: Terminal Screen Cell Grid Architecture & Escape Leakage Elimination

## 1. Executive Summary & Problem Description

When users typed single characters in the integrated PowerShell terminal (e.g. typing `t` or `a`), the terminal rendered leaked ANSI escape fragments inline with user input:
- Typing `t` displayed: `t37m`
- Typing `a` displayed: `ampas\[97m`

This defect rendered interactive typing in PowerShell unusable because PSReadLine syntax highlighting and inline history predictions routinely interleave ANSI SGR styling and cursor movement around typed characters.

---

## 2. Root Cause Analysis

### 2.1 The String-Based Buffer Invalidation
Previously, `TerminalBuffer` stored terminal screen rows as raw `String`s with inline ANSI sequences (e.g. `\x1b[93m`, `\x1b[37m`).
However, terminal cursor movement commands (`CSI H`, `CSI G`, cursor forward/back) manipulate **visual 2D character column positions** (e.g. column 38).

When PSReadLine:
1. Emitted `\x1b[93m` (yellow color code for syntax token).
2. Emitted `t`.
3. Emitted inline prediction in dimmed text: `\x1b[97m\x1b[2m\x1b[3memp-terminal...`
4. Emitted cursor return: `\x1b[4;39H` (jump cursor back to column 38, right after `t`).
5. Emitted color reset: `\x1b[37m` or `\x1b[m`.

When subsequent characters were typed at visual column 38, `TerminalBuffer` attempted string replacement using `.char_indices().nth(self.cursor_col)`.
Because the string contained hidden ANSI escape characters (`\x1b`, `[`, `9`, `3`, `m`), visual column 38 pointed directly into the middle of an escape sequence (`\x1b[`).
Replacing character 38 sliced away `\x1b[`, leaving the remaining numeric parameters and final command character: **`37m`** or **`[97m`**.
These orphaned characters were then treated as ordinary printable text and rendered on the screen.

---

## 3. The Solution: Fixed 2D `ScreenCell` Grid Architecture

Instead of mixing raw escape codes with printable characters in raw strings:
1. **`ScreenCell` Model:**
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct ScreenCell {
       pub ch: char,
       pub fg: u16,        // Current ANSI foreground SGR code
       pub bg: u16,        // Current ANSI background SGR code
       pub bold: bool,
       pub italic: bool,
       pub underline: bool,
   }
   ```
2. **Fixed 2D Grid Representation (`screen: Vec<Vec<ScreenCell>>`):**
   - Each terminal screen line is an indexed row of `cols` distinct `ScreenCell` elements.
   - The cursor position `(cursor_row, cursor_col)` directly indexes `screen[cursor_row][cursor_col]`.
   - Modifying a cell at `cursor_col` updates that cell's character and styling in $O(1)$ without affecting any adjacent cells or parsing state.
3. **Pristine Line Serialization (`line_to_string`):**
   - When scrolling off-screen or returning view lines, the cell grid row is serialized into an optimal, clean ANSI string.
   - SGR state changes are batched efficiently across runs of identical cells, with proper resets.
   - Escape sequences are **never** present in the screen grid, guaranteeing that cursor positioning, erasing (`CSI K`, `CSI X`), and character overwrites can **never corrupt or leak escape codes**.

---

## 4. Verification & Windows Acceptance Evidence

### 4.1 Automated Unit & Integration Tests
- `test_terminal_buffer_psreadline_escape_line_clearing`: Validates prompt erasure with ECH.
- `test_terminal_buffer_ech_erase_characters_and_csi_k`: Validates prompt + long command + ECH + CSI K.
- `test_terminal_buffer_csi_and_utf8_split_at_all_chunk_boundaries`: Validates chunk splitting across multi-byte boundaries.
- **Full Workspace Test Suite:** 17 UI integration tests passed; all 186 workspace tests passed (0 failed).

### 4.2 Windows Desktop Acceptance Test
Tested on the live Windows desktop using the release build of `temp-terminal-gui.exe`:
1. Pressed `t`:
   - Typed `t` displayed cleanly as `t` with inline prediction `testing` (properly styled, zero `37m` leakage; captured in `test_t_key_press.png`).
2. Pressed Backspace and pressed `a`:
   - Typed `a` displayed cleanly as `a` with inline prediction `ampas\` (properly styled, zero `[97m` leakage; captured in `test_a_key_press.png`).

---

## 5. Distributable Artifact Inventory & SHA-256 Checksums

The release archive was regenerated with `--release --workspace --locked` (Rust 1.83.0 MSRV):

| Artifact File | Size (Bytes) | SHA-256 Checksum |
|---|---|---|
| `temp-terminal-gui.exe` | 6,063,616 | `7FE0047D6F38C02CA28704E07F8F32EF25B70918890C6ACD317CAEEE7ADDF868` |
| `temp-terminal-daemon.exe` | 2,003,456 | `827DBB3CD91F012D5F7A58D401D423DEFEC5E60B6ED261EACC1FC237B16FF1F0` |
| `temp-terminal.exe` | 1,261,056 | `FF3D667DAA500ABF3E9A665A2F64E5989E251DFCE6CB1118126962BD3D87F632` |
| `README.txt` | 1,191 | `149666F947CD8EE82DD7E2FF61DE6667A6C97FD4FD60878F7F8B1E5977CD3B3F` |
| **`temp-terminal-windows-x64.zip`** | **3,838,570** | **`3531C183C28D43B6FAFFBB8A195C9BE80728AA23A68F4D14023CDCB0604288B6`** |

---

## 6. Git Provenance

- **Branch:** `feat/p1-02-core-proto`
- **Parent Commit:** `85782be37c8d46f025a2a41f1723f7315a11a640`
- **Commit Subject:** `fix(ui): eliminate ansi escape leakage via screen cell grid architecture`
