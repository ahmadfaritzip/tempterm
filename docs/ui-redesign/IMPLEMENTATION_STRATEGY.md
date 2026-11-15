# TEMP-TERMINAL UI/UX REDESIGN: TECHNICAL IMPLEMENTATION STRATEGY

**Target Crate:** `temp-terminal/crates/temp-terminal-ui/`  
**Document Version:** `v1.0.0`  
**Date:** 2026-11-08  
**Status:** ARCHITECTURAL BLUEPRINT (Implementation Deferred Pending User Sign-Off)  

---

## 1. Architectural Scope & Boundary Enforcement

The redesign is strictly a **frontend presentation overhaul**. The system boundaries are established as follows:

```
+-----------------------------------------------------------------------------------+
|                            GUI LAYER (MODIFIED IN PHASE 2)                        |
|   temp-terminal-ui (workspace.rs, renderer.rs, editor.rs, settings.rs)             |
|   - Visual Theme & Semantic Tokens                                                |
|   - Component Layouts (Tabs, Sidebar, Inspector, Status Bar, Split Panes)         |
|   - Modal Dialogs (Master-Detail Settings, Command Palette, Forms)                |
+-----------------------------------------------------------------------------------+
                                         │
                                         ▼ (UNMODIFIED CONTRACTS)
+-----------------------------------------------------------------------------------+
|                        BACKEND & PROCESS ENGINE (READ-ONLY)                       |
|   - temp-terminal-core   (DevURL proxy, Session Registry, Mailbox, Git operations)|
|   - temp-terminal-daemon (TCP JSON-RPC IPC Server, Loopback Security)             |
|   - temp-terminal-procs  (Supervisor, Restart Backoff, Port Probing, Watcher)     |
|   - temp-terminal-pty    (Windows ConPTY, I/O streaming, Resize)                  |
|   - temp-terminal-term   (VT Parser, OSC Records, 2D ScreenCell Grid)             |
|   - temp-terminal-proto  (Binary framing, Base64, JSON-RPC DTOs)                  |
+-----------------------------------------------------------------------------------+
```

### Invariant Rules
1. **Zero changes to IPC and DTO protocols.**
2. **Zero changes to PTY, ConPTY, or VT escape parsing.**
3. **Zero changes to keyboard shortcuts dispatcher:**
   - `Ctrl+B` (Toggle Sidebar)
   - `Ctrl+T` (New Terminal Pane)
   - `Ctrl+W` (Close Terminal Pane)
   - `Ctrl+O` (Open Project)
   - `Ctrl+P` / `Ctrl+K` (Command Palette)
   - `Ctrl+Shift+F` (Quick Open File)
   - `Ctrl+Shift+Z` (Zoom Active Pane)
   - `Ctrl+,` (Open Settings)
   - `F1`–`F12` (Functional overlays & editor actions)
4. **`omaterm/` Go directory remains strictly READ-ONLY** (`git diff omaterm/` = 0 bytes).
5. **`RUST REWRITE - TODO.md` remains untracked and untouched.**

---

## 2. Refactoring Phases & Step-by-Step Roadmap

When user approval is granted to begin implementation, the work will proceed through 5 decoupled phases:

### Phase A: Theme Foundation & Semantic Tokens (`workspace.rs::theme`)
- Expand `pub mod theme` with the "Obsidian / Linear Slate" palette:
  - Surface layers: `SURFACE_CANVAS`, `SURFACE_PANEL`, `SURFACE_CARD`, `SURFACE_ELEVATED`, `SURFACE_INPUT`, `SURFACE_OVERLAY`.
  - Text scale: `TEXT_PRIMARY`, `TEXT_SECONDARY`, `TEXT_MUTED`, `TEXT_DISABLED`.
  - Border scale: `BORDER_SUBTLE`, `BORDER_MEDIUM`, `BORDER_FOCUS`.
  - Accent colors: `ACCENT_PRIMARY`, `ACCENT_HOVER`, `ACCENT_SUBTLE`.
  - Status colors: `STATUS_SUCCESS`, `STATUS_WARNING`, `STATUS_ERROR`, `STATUS_INFO`.
- Maintain backward-compatible aliases (`BG_APP`, `BG_CARD`, `TEXT_STRONG`, `BORDER`, etc.) so that transitions can be tested incrementally without broken references.
- Configure global `ctx.set_visuals` with adjusted dark mode defaults (panel fills, window rounding, item spacing).

### Phase B: Structural Frame Chrome (Top Bar, Status Bar, Sidebar)
1. **Top Bar (`render_topbar`):**
   - Replace pill buttons with rectangular segmented tabs (`rounding: 4.0px`).
   - Add active tab bottom highlight bar (`ACCENT_PRIMARY`).
   - Group action buttons (`+ New`, Split, Inspector, Settings) into a cohesive tool cluster.
   - Clean up breadcrumb display (`temp-terminal / <Project>`).
2. **Status Bar (`render_statusbar`):**
   - Implement chip badges for Git branch, CWD, active daemon connection, and zoom status.
   - Right-align terminal dimensions and keyboard hints.
3. **Sidebar (`render_sidebar`):**
   - Implement collapsible section cards with styled headers (`▾ SESSIONS`).
   - Redesign project card with Git branch indicator.
   - Redesign process cards with clean status dots (green/amber/red) and port pills (`:3000`).

### Phase C: Workspace Grid & Split Panes
1. **Split Dividers (`render_split_tree`):**
   - Refine the 8px split divider with a centered 30px × 4px capsule pill grip.
   - Accent highlight (`ACCENT_PRIMARY`) on hover or active dragging.
2. **Pane Containers (`render_single_pane`):**
   - Add explicit `BORDER_FOCUS` outline to the active focused pane.
   - Streamline the pane header: Shell icon + Session Title + PID badge + Zoom & Close controls.
3. **Empty State (`render_empty_state`):**
   - Replace basic text cards with 3 horizontal pro-developer action cards with keyboard chips (`F4`, `F6`, `Ctrl+T`).

### Phase D: Dialogs & Overlays Overhaul
1. **Modal Backdrop:**
   - Add full-screen 40% black dimming frame behind modals to create visual depth and isolate focus.
2. **Settings Modal (`Ctrl+,`):**
   - Transform the wrapped 11-button bar into a clean **Two-Column Master-Detail Layout**:
     - Left column: vertical list of categories (General, Terminal, Editor, Processes, etc.).
     - Right column: scrollable configuration content with styled switches, inputs, and sliders.
3. **Command Palette (`Ctrl+P` / `Ctrl+K`):**
   - Refine the centered spotlight bar with search icon, subtle drop shadow, and shortcut badges.
4. **New Agent & Add Process Forms:**
   - Group inputs with clear section labels and helper text.

### Phase E: Editor, Markdown Preview & Diff Viewer
1. **Editor Pane (`render_editor_pane`):**
   - Crisp line number gutter with active line highlight.
   - Tab header with file icon, dirty indicator, and close button.
2. **Markdown Preview (`render_markdown_view`):**
   - 720 DIP centered column with pro typography.
   - Styled code blocks, callout blockquotes, and headings.
3. **Diff Viewer (`render_diff_viewer`):**
   - Refined staging toolbar with file stats chips (`+14`, `-3`).
   - Modern stage/unstage buttons and two-step discard confirmation prompt.

---

## 3. Testing & Verification Plan

Each phase will be guarded by strict verification steps:
1. `cargo check --workspace --locked` (0 warnings).
2. `cargo test --workspace --locked -- --test-threads=1` (All 220 tests pass).
3. `cargo clippy --workspace --all-targets --locked -- -D warnings` (0 clippy warnings).
4. `cargo fmt --all -- --check` (0 diff).
5. Interactive runtime verification script to capture screenshots of each redesigned component at 1440x900 DIP.
6. Git boundary check: `git diff omaterm/` must remain exactly 0 bytes.
