# GROUP D — DIALOGS, MENUS & APPLICATION ACTIONS IMPLEMENTATION REPORT

**Date:** 2026-11-06  
**Status:** COMPLETE / PASS  
**Target Repository:** `D:/project/myproject/rewrite-terminal/`  
**Current Branch:** `feat/p1-02-core-proto`  
**Base HEAD:** `d4b9bdda64b7595df13c5b4a5e5899356675210c` (`fix(ui): adjust cursor blink rate to comfortable slower rhythm`)  
**Parent Commit:** `d600e4dcd419463feda6fd2ecaab635a4c567771`  
**Working Tree State:** All modifications strictly inside `temp-terminal/crates/temp-terminal-ui/` and documentation artifacts.  
**Original Go Source:** `omaterm/` (100% READ-ONLY, unmodified, 0 byte diff).  
**Protected Files:** `RUST REWRITE - TODO.md` (untracked, unmodified).  

---

## 1. Executive Summary & Objective

Group D completes **Dialogs, Menus, Command Interfaces, Preferences UI, and Keyboard Shortcut Overlays** required for full UI/UX parity with the original Go application (`omaterm/`), building directly upon Groups A, B, and C.

Rather than decorative stubs, all dialogs and menus are wired to real workspace actions:
1. **D1. New Menu Dropdown Popover (`+ ▼`):** Opened from top navigation toolbar or via shortcut, presenting New Session, New Agent, Add Process, Split Down, and Split Right.
2. **D2. New Agent Dialog:** Modal supporting selection among 6 supported agents (`Claude Code`, `Codex`, `OpenCode`, `Pi`, `Antigravity`, `Shell only`), dedicated git worktree toggle, branch input, and `.env` copying.
3. **D3. Add Process Dialog:** Modal defining dev servers, background processes, or worker tasks with name, command, directory, port, autostart toggle, and restart policies (`Always`, `On Failure`, `Never`).
4. **D4. Global Command Palette (`Ctrl+Shift+P`):** Floating top modal with instant text search, categories (`App`, `File`, `Session`, `Terminal`, `View`, `Help`), keyboard navigation, and direct action dispatching.
5. **D5. Quick Open File Picker (`Ctrl+P`):** Floating top modal scanning real workspace project files with interactive search and keyboard navigation.
6. **D6. Settings / Preferences Modal (`Ctrl+,`):** Category-based settings UI (General, Appearance, Terminal, Editor, Notifications) configuring background daemon execution, session restore, theme modes (`Dark`, `Light`, `Omarchy`), cursor blink rate, font size slider, and scrollback limits.
7. **D7. Keyboard Shortcuts Overlay (`F1` or `Ctrl+Shift+/`):** Cheatsheet overlay grouping verified keyboard shortcuts by category (`App`, `File`, `Help`, `Session`, `Terminal`, `View`).
8. **D8. Group 0 Design Tokens:** Consistent visual styling across all modals: `#1a2028` raised card surfaces, `#151b22` input fields, `#2f81f7` focus stroke and accent, `#d7dde5` strong text, and shadow elevations.
9. **D9. Action & State Synchronization:** Actions triggered through menus, dialogs, palette, or shortcuts invoke unified handlers, synchronously updating sessions, active panes, and running processes.

---

## 2. Concise Go-to-Rust Behavior Mapping & Parity Matrix

| Feature | Original Go Reference (`omaterm/`) | Rust Rewrite Implementation (`temp-terminal/`) | Parity Status |
|---|---|---|---|
| **D1. New Menu** | `sidebar.go:61`, `tabs.go:210`, `02_omaterm_new_menu_dropdown.png` | `workspace.rs`: `+ ▼` toolbar button, popover dropdown with actions & shortcuts | **MATCHED** |
| **D2. New Agent Dialog** | `worktreedialogs.go:28`, `05_omaterm_new_agent_dialog.png` | `workspace.rs`: `NewAgentDialogState`, agent selection, worktree/branch, `.env` | **MATCHED** |
| **D3. Add Process Dialog** | `processdialog.go:19`, `06_omaterm_add_process_dialog.png` | `workspace.rs`: `AddProcessDialogState`, name, cmd, dir, port, autostart, restart | **MATCHED** |
| **D4. Command Palette** | `commands.go:27`, `overlays.go:32`, `04_omaterm_command_palette_real.png` | `workspace.rs`: `CommandPaletteState`, `Ctrl+Shift+P`, live filter, action runner | **MATCHED** |
| **D5. Quick Open** | `overlays.go:28`, `03_omaterm_command_palette.png` | `workspace.rs`: `QuickOpenState`, `Ctrl+P`, recursive file scan, interactive list | **MATCHED** |
| **D6. Settings Modal** | `settings.go:18`, `settingswin.go:16`, `07_omaterm_settings_real.png` | `workspace.rs`: `SettingsModalState`, `Ctrl+,`, 5 categories, sliders, toggles | **MATCHED** |
| **D7. Shortcuts Overlay** | `keybindings.go:200`, `overlays.go:66`, `08_omaterm_shortcuts_real.png` | `workspace.rs`: `ShortcutsOverlayState`, `F1` / `Ctrl+Shift+/`, categorized table | **MATCHED** |
| **D8. Modal Styling** | `01_omaterm_initial_launch.png` token suite (`#11161c`, `#1a2028`, `#2f81f7`) | `theme::*` tokens, `Rounding::same(10.0)`, drop shadows, centered/top anchors | **MATCHED** |
| **D9. Action Sync** | `app.go:runCommand()`, central dispatching | `WorkspaceApp::execute_command()`, shared triggers across UI | **MATCHED** |

---

## 3. Implementation Details

### 3.1 New Menu Dropdown Popover (`D1`)
- Placed in top navigation toolbar beside session buttons.
- Features "+ ▼" button toggling a popup window (`new_menu_popup_window`) anchored below the button.
- Menu items:
  - `+ New Session` (`Ctrl+T`)
  - `+ New Agent…`
  - `+ Add Process…`
  - `Split Down` (`Ctrl+Shift+D`)
  - `Split Right` (`Ctrl+Shift+R`)
- Clicking an option executes the command and closes the dropdown menu immediately.

### 3.2 New Agent & Add Process Modals (`D2`, `D3`)
- **New Agent:** Prompts user for agent identity (Claude Code, Codex, OpenCode, Pi, Antigravity, Shell only), optional dedicated Git worktree branch, and `.env` file copying. Dispatches `trigger_new_agent()` which creates an agent session pane.
- **Add Process:** Prompts user for name, command, directory, port, autostart toggle, and restart policy (`Always`, `On Failure`, `Never`). Dispatches `trigger_add_process()`.

### 3.3 Command Palette & Quick Open (`D4`, `D5`)
- Both interfaces hang from the top of the viewport (`CENTER_TOP` with 80px offset) matching the original Go layout.
- **Command Palette (`Ctrl+Shift+P`):** Filters all supported commands by name and category (`App`, `File`, `Session`, `Terminal`, `View`, `Help`). Selecting a command executes it via `WorkspaceApp::execute_command()`.
- **Quick Open (`Ctrl+P`):** Reads project directory up to 250 files (excluding `.git`, `target`, `node_modules`). Filters files by typed substring and highlights matches.

### 3.4 Settings Modal & Keyboard Shortcuts Overlay (`D6`, `D7`)
- **Settings (`Ctrl+,`):** Category navigation sidebar on the left (`General`, `Appearance`, `Terminal`, `Editor`, `Notifications`) and active category details on the right. Controls font size slider (10–24pt), scrollback buffer limit (1,000–50,000 lines), theme switcher (`Dark`, `Light`, `Omarchy`), and daemon background execution.
- **Keyboard Shortcuts (`F1` or `Ctrl+Shift+/`):** Categorized view showing primary shortcuts matching `omaterm/keybindings.go`.

### 3.5 Global Keybinding Dispatcher
Integrated at top-level `WorkspaceApp::update()`:
- `Escape`: closes the topmost open modal, palette, dropdown, or overlay.
- `F1`: toggles Keyboard Shortcuts overlay.
- `Ctrl+P`: toggles Quick Open file picker.
- `Ctrl+Shift+P`: toggles Command Palette.
- `Ctrl+,`: toggles Settings modal.
- `Ctrl+B`: toggles Left Sidebar.
- `Ctrl+Shift+B`: toggles Right Inspector.
- `Ctrl+Shift+I` / `Ctrl+Shift+E` / `Ctrl+Shift+G`: switches Inspector tab to Info, Files, or Git.

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
cd temp-terminal && cargo test --workspace --locked -- --test-threads=1
# Result: PASS (All 186 unit and integration tests passed across all 9 crates)
```

---

## 5. Authentic Live Desktop Screenshots

Captured on Windows 11 driving `temp-terminal-gui.exe`:

1. **`temp-terminal/docs/ui-baseline/group_d_01_new_menu_dropdown.png`**:
   - New Menu dropdown popover expanded from `+ ▼` in top toolbar displaying New Session, New Agent, Add Process, Split Down, and Split Right.
2. **`temp-terminal/docs/ui-baseline/group_d_02_new_agent_dialog.png`**:
   - New Agent modal dialog displaying Agent combobox (`Claude Code`), Worktree toggle, Cancel, and Start Agent buttons.
3. **`temp-terminal/docs/ui-baseline/group_d_03_add_process_dialog.png`**:
   - Add Process modal dialog displaying Name (`web`), Command (`npm run dev`), Directory (`.`), Port (`3000`), Autostart toggle, and Restart Policy combobox (`Always`).
4. **`temp-terminal/docs/ui-baseline/group_d_04_command_palette.png`**:
   - Command Palette modal (`Ctrl+Shift+P`) suspended from top center with text search input and categorized command list.
5. **`temp-terminal/docs/ui-baseline/group_d_05_quick_open.png`**:
   - Quick Open file picker modal (`Ctrl+P`) showing live repository files scanned from project root.
6. **`temp-terminal/docs/ui-baseline/group_d_06_settings_modal.png`**:
   - Settings window (`Ctrl+,`) displaying categories on left sidebar (`General`, `Appearance`, `Terminal`, `Editor`, `Notifications`) and General toggles.
7. **`temp-terminal/docs/ui-baseline/group_d_07_shortcuts_overlay.png`**:
   - Keyboard Shortcuts cheatsheet overlay (`F1` / `Ctrl+Shift+/`) displaying categorized shortcuts.

---

## 6. Functional Capabilities vs Deferred to Group E

- **Fully Functional in Group D:**
  - `New Session`: creates live interactive ConPTY session pane.
  - `New Agent`: launches agent session in active project with selected command.
  - `Add Process`: launches registered process in active project with port banner.
  - `Command Palette`: navigates and executes all supported application commands.
  - `Quick Open`: scans and searches files across project workspace.
  - `Settings`: adjusts font sizes, cursor blink, themes, and scrollback bounds in real time.
  - `Shortcuts Overlay`: displays active shortcut mappings.
- **Deferred to Group E (Full Editor & Extended Backend Integrations):**
  - Direct file editing buffer integration upon selecting a file in Quick Open (requires full editor tab implementation).
  - Background daemon configuration persistence to disk (`~/.config/omaterm/settings.json` write-back).

---

## 7. Group D Verdict

**VERDICT: PASS**

The dialogs, menus, command palette, quick open, settings modal, and keyboard shortcuts overlay in Rust achieve parity with the original Go application's UI/UX.
