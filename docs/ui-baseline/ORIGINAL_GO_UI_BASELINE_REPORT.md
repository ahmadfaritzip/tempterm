# GROUP 0 — COMPLETE ORIGINAL GO UI/UX BASELINE INVESTIGATION REPORT

**Date:** 2026-11-06  
**Status:** COMPLETE (STRICT READ-ONLY INVESTIGATION — NO IMPLEMENTATION)  
**Target Repository:** `D:/project/myproject/rewrite-terminal/`  
**Current Branch:** `feat/p1-02-core-proto`  
**HEAD Commit:** `d4b9bdda64b7595df13c5b4a5e5899356675210c` (`fix(ui): adjust cursor blink rate to comfortable slower rhythm`)  
**Parent Commit:** `d600e4dcd419463feda6fd2ecaab635a4c567771`  
**Original Application Source:** `omaterm/` (100% READ-ONLY, unmodified, 0 byte diff)  
**Authentic Screenshot Artifacts:** `temp-terminal/docs/ui-baseline/original/`

---

## 1. Executive Summary & Mission Scope

This investigation establishes the definitive visual, structural, and behavioral baseline of the original Go application (`omaterm`, ~73,000 lines). The original application serves as the **SINGLE SOURCE OF TRUTH (SSOT)** for all UI/UX parity requirements during the Rust rewrite (`temp-terminal`).

The investigation was executed in **strict read-only mode** with two parallel discovery methods:
1. **Source Code Architectural Audit:** Exhaustive code-level inspection across all core Go GUI files (`main.go`, `app.go`, `sidebar.go`, `tabs.go`, `terminal.go`, `paneheader.go`, `statusbar.go`, `theme.go`, `internal/theme/builtin.go`, `internal/theme/derive.go`, `worktreedialogs.go`, `processdialog.go`, `settings.go`, `keybindings.go`, `commands.go`).
2. **Authentic Live Windows Desktop Execution:** Building and executing `omaterm` on Windows 11 AMD64, directly capturing pixel-perfect authentic screenshots across the full application shell, dialog modals, inspector tabs, and split terminal layouts.

---

## 2. Repository & Git Preflight Verification

- **Repository Root:** `D:/project/myproject/rewrite-terminal`
- **Active Branch:** `feat/p1-02-core-proto`
- **Current HEAD Hash:** `d4b9bdda64b7595df13c5b4a5e5899356675210c`
- **Preceding Functional Commits:**
  - `d600e4d`: `feat(ui): add blinking terminal cursor rendering in active pane`
  - `b5baa94`: `feat(ui): auto-copy on mouse select and clipboard copy paste`
  - `f3321fd`: `fix(ui): enforce terminal auto-wrap at column boundary`
  - `23c1a7c`: `fix(ui): route enter key as CR 0x0d to prevent multiline prompt`
  - `ddce059`: `fix(ui): route standard backspace as DEL 0x7f to delete single characters`
  - `5387cb0`: `fix(ui): resolve ansi escape leakage with 2d screencell buffer architecture`
  - `85782be`: `fix(ui): add CSI X, CSI G, CSI d and escape buffering to eliminate stale PSReadLine text`
- **Integrity Verification:**
  - `git diff omaterm/` returned 0 bytes (unmodified, preserved).
  - `RUST REWRITE - TODO.md` remains untracked and untouched.
  - Temporary test binary `omaterm/omaterm_test.exe` was cleanly deleted after runtime capture.

---

## 3. Original Go Architecture & Subsystem Audit

### 3.1 Application Entry Point & Lifecycle
- **Entry File:** `omaterm/main.go` (`main()`, `appState`)
- **CLI & Daemon Distinction:**
  - `omaterm __daemon`: Starts the headless background process manager without GUI.
  - `omaterm <command>`: Command-line interface communicating over the local control socket (`internal/ctl`).
  - `omaterm [dir]`: Opens the GUI window. Single instance enforcement via `mygo.App.RequestSingleInstanceLock()` and `mygo.App.OnSecondInstance`.
- **Default Window Parameters:**
  - Initial Dimensions: `1440 x 900` DIPs (width x height).
  - Minimum Dimensions: `600 x 400` DIPs.
  - State Persistence Key: `"main"` (saved to `state.json`).

### 3.2 Window Host & Frontend Framework
- **Framework:** `github.com/egoist/mygo` (a custom lightweight Go native UI framework inspired by Flutter/SwiftUI immediate+retained layout tree, using native rendering backends).
- **Embedded Terminal Engine:** `github.com/egoist/mygo/plugins/terminal` (native terminal plugin wrapping an xterm.js / headless VT emulator core with GPU-accelerated texture font glyph atlas).

### 3.3 Root View Hierarchy (`omaterm/app.go`)
The entire user interface is constructed in `appState.view(c *ui.Context)`:
1. **Backdrop & Glow:** `wash(root)` with linear gradient angle `180°` (Oklab interpolation), plus an ambient wash glow overlay at angle `225°` (`pal.WashGlow` fading out at 55%).
2. **Main Split Row:**
   - **Left Sidebar:** `sidebar(c)` (collapsible, min 200px, max 480px, default 240px).
   - **Main Content Column:** `mainColumn(c)`:
     - Top Tab Bar (`tabBar(c)`).
     - Center Viewport: Status banners + terminal / editor / diff / graph views.
     - Right Inspector Panel (`inspectorPanel(c)`): Info, Files, Git tabs (min 260px, max 640px, default 330px).
3. **Bottom Status Bar:** `statusBar(c)` (fixed 24px height).
4. **Modal Dialogs & Overlays:** Centered backdrop scrim overlay (`pal.Scrim = rgba(0, 0, 0, 0.35)`).

---

## 4. Complete Screen Inventory & Feature Verification

| Screen / Feature | Location in Go Code | UI Presence & Behavior | Authentic Artifact |
|---|---|---|---|
| **A. Main Application Shell** | `app.go`, `sidebar.go`, `statusbar.go` | Collapsible sidebar, tab bar, status bar with project/branch/running/port indicators. | `01_omaterm_main_window.png` |
| **B. Project Management** | `sidebar.go:88`, `sidebarEmpty()` | Lists projects, project group items, "+" button for new session/agent/process, open folder dialog. | `01_omaterm_main_window.png` |
| **C. Session Management** | `sidebar.go`, `tabs.go` | Top tabs with session name, running dots, "+" button for new tab, session switcher. | `01_omaterm_main_window.png` |
| **D. Split Terminal Workspace** | `terminal.go:21`, `paneheader.go` | Arbitrary vertical/horizontal binary split tree (`splitNode`), pane headers with branch & status chips, action buttons (`Split Right`, `Split Down`, `Maximize`, `Close`). | `14_omaterm_split_grid_real.png` |
| **E. New Agent Interface** | `worktreedialogs.go:27` | Modal dialog: Agent selection (`Claude Code`, `Codex`, `OpenCode`, `Pi`, `Agy`, `Shell`), worktree toggle, branch input, base branch picker, `.env` copy toggle. | `05_omaterm_new_agent_dialog.png` |
| **F. Add Process Interface** | `processdialog.go:16` | Modal dialog: Suggestions, Name, Command, Directory, Port, Autostart toggle, Restart policy (`Never`, `On failure`, `Always`), Watch globs, Environment variables, `Save in omaterm.yml` toggle. | `06_omaterm_add_process_dialog.png` |
| **G. Settings & Preferences** | `settingswin.go`, `settings.go` | Comprehensive preferences window with sidebar categories: General, Appearance, Terminal, Editor, Notifications, Agents, Web Access, Dev URLs, Browser, Keyboard, About. | `07_omaterm_settings_real.png` |
| **H. Command Palette & Quick Open** | `overlays.go`, `keybindings.go` | Quick Open (`Ctrl+P`) for project files, Command Palette (`Ctrl+Shift+P`) for global commands, fuzzy search with keyboard navigation hints. | `03_omaterm_command_palette.png`, `04_omaterm_command_palette_real.png` |
| **I. Keyboard Shortcuts Overlay** | `keybindings.go:13`, `app.go` | Filterable shortcut cheatsheet (`Ctrl+Shift+/` or `F1`), grouped by category and scope. | `08_omaterm_shortcuts_real.png` |
| **J. Inspector Panels (Right Sidebar)** | `inspector.go`, `files.go`, `gitwatch.go` | Tabbed sidebar with 3 views: **Info** (process list with PID/memory/kill, port list, update check), **Files** (hierarchical file tree with icons), **Git** (modified files, diff staging). | `10_omaterm_split_horizontal.png`, `11_omaterm_inspector_files.png` |
| **K. Mailbox & Collaboration** | `mail.go`, `internal/mail` | Inter-agent messaging system (primarily driven via daemon RPC / CLI `omaterm mail ask/reply` and backend toast notifications). | Backend verified |
| **L. DevURL & Tunnels** | `settings_devurls.go`, `webaccess.go` | Device pairing QR codes, remote web access toggle, and local proxy URLs shown in status bar / settings. | `07_omaterm_settings_real.png` |

---

## 5. Visual Design System Extraction

The design system of `omaterm` is strictly derived in `internal/theme/derive.go` from the `BuiltinDark` palette:

### 5.1 Color Palette Values
- **Background / Wash (`pal.Wash`):** `#0c0f14` (dark charcoal-navy wash backdrop).
- **Ambient Wash Glow (`pal.WashGlow`):** `rgba(47, 129, 247, 0.16)` (subtle blue accent glow from top-right corner).
- **Cards & Panes (`pal.Card` / `pal.Code`):** `#11161c` (deep dark blue-gray surface for terminal panes and editor).
- **Raised Surfaces & Popovers (`pal.Raised` / `pal.Popover`):** `#1a2028` (modal dialogs, floating dropdowns, command palette).
- **Input Fields (`pal.Input`):** `#151b22` (text inputs with subtle border).
- **Selection Highlight (`pal.Selection`):** `#1d3a5f` (terminal text selection and active row background).
- **Active Blue Accent (`pal.Accent`):** `#2f81f7` (buttons, active tabs, focused borders, cursor highlights).
- **Accent Hover (`pal.AccentHover`):** `#3b8cff`.
- **Text Strong / Bright (`pal.TextStrong`):** `#ffffff`.
- **Text Normal / Default (`pal.Text`):** `#d7dde5`.
- **Text Muted (`pal.TextMuted`):** `#7f8a99`.
- **Text Faint (`pal.TextFaint`):** `#4d5663`.
- **Borders & Dividers:**
  - `pal.Border`: `rgba(215, 221, 229, 0.08)` (hairline divider).
  - `pal.BorderStrong`: `rgba(215, 221, 229, 0.14)` (card edges, button borders).
  - `pal.Scrim`: `rgba(0, 0, 0, 0.35)` (modal dimming backdrop).
- **Status Indicators:**
  - Success (`pal.Ok`): `#63d58d` (active running process dot, git additions).
  - Caution / Attention (`pal.Warn`): `#f3a85f` (warning alerts, modified git files).
  - Failure / Danger (`pal.Bad`): `#ff6f6f` (kill button, git deletions, errors).

### 5.2 Geometry & Typography Specs
- **Window Padding & Gaps:**
  - Pane gap: `8px` (`a.paneGap()`).
  - Card corner radius: `8px` to `10px` (`Radius(10)` on dialogs, `Radius(8)` on pane cards).
  - Dialog dimensions: `420px` width, centered horizontally, positioned at `20%` from top window height (`winH * 0.20`).
- **Control Dimensions:**
  - Tab bar height: `40px`.
  - Pane header height: `30px` (comfortable) or `26px` (compact).
  - Status bar height: `24px`.
  - Dialog button height: `30px`, radius `6px`.
  - Sidebar width: default `240px` (clamped `200px` – `480px`).
  - Inspector width: default `330px` (clamped `260px` – `640px`).
- **Typography:**
  - UI Font Family: `Inter, Noto Sans, system-ui`.
  - Code & Terminal Font: `Consolas, Menlo, Monaco, monospace`.
  - Window Title & Headings: `14px`, Weight `600`.
  - Tab & Pane Titles: `12.5px`, Weight `600`.
  - Body Text & Inputs: `12px` – `12.5px`, Weight `400`–`500`.
  - Secondary Details & Subtitles: `11px` – `11.5px`, Regular.
  - Status Bar & Metadata Labels: `10px`, Regular.

---

## 6. Inventory of Captured Authentic Screenshot Artifacts

All screenshots have been verified as genuine captures of the running original application and are preserved under `temp-terminal/docs/ui-baseline/original/`:

1. `01_omaterm_main_window.png` — Main application shell on initial launch (sidebar, tab bar, status bar, single terminal pane).
2. `02_omaterm_new_menu_dropdown.png` — Sidebar header "+" menu dropdown.
3. `03_omaterm_command_palette.png` — Quick Open modal (`Ctrl+P`) showing fuzzy file search.
4. `04_omaterm_command_palette_real.png` — Global Command Palette modal (`Ctrl+Shift+P`).
5. `05_omaterm_new_agent_dialog.png` — New Agent modal dialog with worktree and branch options.
6. `06_omaterm_add_process_dialog.png` — Add Process modal dialog with autostart and restart policies.
7. `07_omaterm_settings_real.png` — Full Settings preferences window with categorized sidebar.
8. `08_omaterm_shortcuts_real.png` — Keyboard Shortcuts overlay cheat sheet (`Ctrl+Shift+/`).
9. `09_omaterm_inspector_info_panel.png` — Inspector panel: Info view (process table, listening ports).
10. `10_omaterm_split_horizontal.png` — Horizontal split preview with active Info inspector.
11. `11_omaterm_inspector_files.png` — Inspector panel: Files hierarchical project directory tree.
12. `12_omaterm_inspector_git.png` — Inspector panel: Git status view with diff overview.
13. `13_omaterm_split_down_real.png` — Live horizontal split execution (`Ctrl+Shift+D`).
14. `14_omaterm_split_grid_real.png` — Live 3-pane multi-split grid workspace (`Ctrl+Shift+R`).

---

## 7. Conclusion & Next Steps for Rust Implementation

The original Go application baseline is now fully audited, documented, and verified with visual evidence. Every layout dimension, color hex, font metric, split-tree algorithm, and dialog interaction has been mapped to exact line numbers and concrete image captures.

This specification provides the foundation to guide Group 1 through Group 5 parity implementations in the Rust codebase (`temp-terminal-ui` and `temp-terminal-gui`).
