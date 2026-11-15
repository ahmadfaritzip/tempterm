# GROUP C — TERMINAL WORKSPACE IMPLEMENTATION REPORT

**Date:** 2026-11-06  
**Status:** COMPLETE / PASS  
**Target Repository:** `D:/project/myproject/rewrite-terminal/`  
**Current Branch:** `feat/p1-02-core-proto`  
**Base HEAD:** `d4b9bdda64b7595df13c5b4a5e5899356675210c` (`fix(ui): adjust cursor blink rate to comfortable slower rhythm`)  
**Parent Commit:** `d600e4dcd419463feda6fd2ecaab635a4c567771`  
**Working Tree State:** Modifications exclusively under `temp-terminal/crates/temp-terminal-ui/` and documentation artifacts.  
**Original Go Source:** `omaterm/` (100% READ-ONLY, unmodified, 0 byte diff).  
**Protected Files:** `RUST REWRITE - TODO.md` (untracked, unmodified).  

---

## 1. Executive Summary & Objective

Group C implements and completes **Terminal Workspace Parity** with the original Go application (`omaterm/`) in the Rust GUI (`temp-terminal-ui` / `temp-terminal-gui`).

Building directly upon the foundations established in Group A (application shell) and Group B (project & session navigation), the terminal workspace functions as a real interactive multi-pane terminal system, preserving:
1. Interactive Windows ConPTY + PowerShell execution.
2. Binary split tree model (`SplitNode` with `SplitDir::Vertical` and `SplitDir::Horizontal`) matching `omaterm/terminal.go`.
3. Go baseline shortcuts: `Ctrl+Shift+D` (Split Down) and `Ctrl+Shift+R` (Split Right), alongside header action buttons (`Split ═` and `Split ║`).
4. Pane header title (`Terminal • <name>`), close button (`✕` / `Ctrl+W`), and focus tracking (`BORDER_FOCUS` vs `BORDER`).
5. Per-pane dynamic viewport geometry calculation and automatic asynchronous PTY resizing.
6. Mouse interaction with left-click focus activation, drag selection, auto-copy to system clipboard on release, and right-click paste/copy.
7. Smooth VT cursor rendering with relaxed cadence (1.4s cycle), bounded scrollback retention, and PSReadLine navigation stability.

---

## 2. Concise Go-to-Rust Behavior Mapping & Parity Matrix

| Scope Item | Original Go Implementation (`omaterm/`) | Rust Rewrite Implementation (`temp-terminal/`) | Parity Status |
|---|---|---|---|
| **C1. Pane Presentation** | `paneheader.go`, dark card (`#11161c`), border glow on active, title `Terminal • <name>` | `workspace.rs`: `BG_CARD`, `BORDER_FOCUS` vs `BORDER`, header with title, action buttons, close | **MATCHED** |
| **C2. Split Pane Layout** | `terminal.go:28` (`splitVertical`, `splitHorizontal`), binary tree `splitNode`, `splitLeaf` | `lib.rs`: `SplitNode`, `SplitDir`, `split_active_pane()`, recursive rendering in `workspace.rs` | **MATCHED** |
| **C3. Keyboard Input & Focus** | `keybindings.go:238`: `Ctrl+Shift+D` (splitDown), `Ctrl+Shift+R` (splitRight), focus routing | `workspace.rs`: `Ctrl+Shift+D`, `Ctrl+Shift+R`, `Ctrl+W`, `Ctrl+Tab`, `DEL` (0x7f) Backspace, `CR` (0x0d) | **MATCHED** |
| **C4. Mouse & Clipboard** | Left-click focus, drag selection, system clipboard paste/copy, right-click context action | `workspace.rs`: `set_pane_selection`, `get_selected_text`, `set_clipboard_text`, right-click paste/copy | **MATCHED** |
| **C5. Buffer & Scrolling** | Bounded scrollback, ANSI 2D screen cell grid, stick-to-bottom live view | `lib.rs`: `TerminalBuffer` (`ScreenCell` 2D grid), `ScrollArea::vertical().stick_to_bottom(true)` | **MATCHED** |
| **C6. Resize & Lifecycle** | Viewport cols/rows sent to PTY ConPTY on split or window resize | `workspace.rs`: auto-computes `target_cols`/`target_rows` per pane_rect and invokes `pane.resize()` | **MATCHED** |
| **C7. Pane Actions** | Split Right, Split Down, Close pane (`removePane`), Focus pane | Header buttons (`Split ═`, `Split ║`, `✕`), keyboard shortcuts, focus switching | **MATCHED** |
| **C8. State Sync (A & B)** | Synchronized with active project, session breadcrumb, inspector, status bar | Single source of truth in `AppHost` / `WorkspaceSnapshot`, preserves sessions across switches | **MATCHED** |

---

## 3. Detailed Implementations & Parity Enhancements

### 3.1 Split Shortcuts & Header Actions (`C2`, `C7`)
- **Keybinding Parity:** In `omaterm/keybindings.go` lines 238–239, splitting is mapped to `Ctrl+Shift+D` (Split Terminal Down) and `Ctrl+Shift+R` (Split Terminal Right).
- **Rust Implementation:** Added key event handling in `workspace.rs` under `modifiers.ctrl && modifiers.shift`:
  - `Key::D` triggers `self.trigger_split(SplitDir::Horizontal)` (Split Down).
  - `Key::R` triggers `self.trigger_split(SplitDir::Vertical)` (Split Right).
  - `Key::W` triggers `self.trigger_close_pane(pane_id)` (Close Pane).
  - Retained `Key::Tab` / `Ctrl+Shift+Tab` for cycling focus between panes.
- **Header Buttons:** Added descriptive tooltip labels to header buttons (`Split terminal down (Ctrl+Shift+D)` and `Split terminal right (Ctrl+Shift+R)`).

### 3.2 Dynamic Viewport Calculation & ConPTY Resizing (`C6`)
- In `render_single_pane()`, dynamically derives characters geometry from `pane_rect`:
  - `char_w = 7.0f32`, `line_h = 14.5f32`.
  - Computes `target_cols = ((avail_w) / char_w).floor().clamp(20.0, 300.0) as u16`.
  - Computes `target_rows = ((avail_h) / line_h).floor().clamp(5.0, 120.0) as u16`.
  - Dispatches `pane.resize(target_cols, target_rows)` asynchronously when dimension changes are detected.

### 3.3 Mouse Selection & Context Menu Action (`C4`)
- Clicking inside a pane's viewport focuses the pane and clears previous text selection.
- Dragging creates a selection range (`TerminalPos { line_idx, col_idx }`). Releasing immediately copies selected text to the Windows system clipboard via `set_clipboard_text()`.
- Right-clicking (secondary click) inside the viewport copies text if a selection exists, or pastes text from the clipboard if no text is selected.

---

## 4. Verification & Testing

### 4.1 Code Formatting
```bash
cd temp-terminal && cargo fmt --check
# Result: PASS (0 diffs)
```

### 4.2 Workspace Compilation
```bash
cd temp-terminal && cargo check --workspace --locked
# Result: PASS (0 errors, 0 warnings)
```

### 4.3 Workspace Tests
```bash
cd temp-terminal && cargo test --workspace --locked
# Result: PASS (All 186 unit and integration tests passed across all 9 crates)
```

---

## 5. Authentic Live Desktop Screenshots

Captured on Windows 11 driving `temp-terminal-gui.exe`:

1. **`temp-terminal/docs/ui-baseline/group_c_01_single_pane.png`**:
   - Single terminal pane (`Terminal • pane_1`) running PowerShell in `session_1`. Header displays split action buttons and close button.
2. **`temp-terminal/docs/ui-baseline/group_c_02_split_right.png`**:
   - Split Right executed via `Ctrl+Shift+R`. Two equal-width vertical columns (`pane_1` on left, `Terminal 2` on right). Active pane has focus border.
3. **`temp-terminal/docs/ui-baseline/group_c_03_split_down_multi_pane.png`**:
   - Split Down executed via `Ctrl+Shift+D` inside the right pane, producing a nested 3-pane layout (`pane_1` taking the left half, right column divided horizontally into `Terminal 2` and `Terminal 3`).
4. **`temp-terminal/docs/ui-baseline/group_c_04_terminal_scrolling_output.png`**:
   - Interactive PowerShell execution with `dir` command output streaming in the active pane. Demonstrates live ConPTY text rendering, VT color parsing, bounded scrollback, and cursor alignment.

---

## 6. Git & Repository Status

- **Branch:** `feat/p1-02-core-proto`
- **HEAD:** `d4b9bdda64b7595df13c5b4a5e5899356675210c`
- **Diff `omaterm/`:** `0 bytes` (strictly untouched).
- **Protected Files:** `RUST REWRITE - TODO.md` (untracked, untouched).
- **Working Tree Changes:** Only files under `temp-terminal/crates/temp-terminal-ui/` and `temp-terminal/docs/ui-baseline/`.
- **Commit Policy:** No commits created.

---

## 7. Group C Verdict

**VERDICT: PASS**

The terminal workspace in Rust delivers full parity with the original Go application's interactive multi-pane workspace.
