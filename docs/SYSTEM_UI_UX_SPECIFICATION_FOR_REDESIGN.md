# SYSTEM UI/UX SPECIFICATION & ARCHITECTURE DOSSIER: TEMP-TERMINAL

> Document Version: 1.0.0  
> Target Audience: Senior UI/UX Designers, Design Systems Engineers, and Frontier AI Models generating redesign prompts and implementation blueprints.  
> Project Name: `temp-terminal` (Complete Rust Native Rewrite of `omaterm`)  
> Technical Core: Rust 2021, `egui` 0.30, `eframe` 0.30, Windows ConPTY, Tokio Async Runtime  

---

## 1. EXECUTIVE SUMMARY & PURPOSE

This document provides a comprehensive, exhaustive technical and visual specification of **`temp-terminal`**, a modern, high-performance, developer-centric terminal workspace and agent orchestration platform natively built in Rust for Windows.

The purpose of this dossier is to serve as the definitive single-source-of-truth reference for **Frontier AI Models** and **System Designers** to formulate actionable, highly detailed, and production-grade **UI/UX redesign prompts, component specifications, and visual design system architectures**.

Every screen, panel, tab, modal dialog, layout calculation, color token, interaction workflow, keyboard shortcut, and rendering constraint is documented herein.

---

## 2. SYSTEM IDENTITY & CORE MISSION

`temp-terminal` is not a generic shell emulator. It is a **Developer Workspace Environment** designed to bridge traditional terminal emulation, process supervision, file editing, and autonomous AI coding agents into a unified, high-frequency graphical interface.

### Key Value Propositions
1. **Ultra-Low Resource Footprint**: Consumes only ~15 to 35 MB of RAM at runtime, compared to 300 to 800 MB in Electron-based terminal workspaces.
2. **Instant Native Startup**: Cold boot to full interactive state in less than 80 milliseconds.
3. **Deterministic 120+ FPS Hardware-Accelerated Rendering**: Pure immediate-mode 2D vector canvas rendering directly to GPU via DirectX/OpenGL through `glow` and `winit`.
4. **Autonomous AI Agent Orchestration**: Native integration with coding agents (`agy`, `claude`, `codex`, `opencode`, `pi`) running in dedicated or shared git worktrees with real-time process monitoring.
5. **Integrated Developer Ergonomics**: Multi-pane terminal splitting, built-in code editor with syntax highlighting, visual git status, file tree inspector, and rig board tracking.

---

## 3. TECHNICAL STACK & ARCHITECTURE

The application is structured as a multi-crate Cargo workspace in Rust:

```
temp-terminal/
├── Cargo.toml                      # Workspace definition (Resolver 2)
└── crates/
    ├── temp-terminal-proto/        # Wire protocol, JSON-RPC serialization, IPC schema
    ├── temp-terminal-pty/          # Windows ConPTY FFI, process lifecycle, signals (Ctrl+C)
    ├── temp-terminal-term/         # Terminal emulation, VT100/ANSI parser, cell grid
    ├── temp-terminal-detect/       # Agent and process signature detection
    ├── temp-terminal-procs/        # Process supervision, runner, restart policy, trust store
    ├── temp-terminal-core/         # State registry, workspace host, project/session graph
    ├── temp-terminal-daemon/       # Background Windows service daemon, IPC server
    ├── temp-terminal-cli/          # Command-line interface and subcommands
    ├── temp-terminal-rig/          # Rig agent board state, tracking, and coordination
    └── temp-terminal-ui/           # egui/eframe graphical workspace frontend
```

### UI Rendering Engine: `egui` and `eframe`
- **Engine Type**: Immediate-Mode GUI (IMGUI).
- **Frontend Framework**: `egui` 0.30.0 / `eframe` 0.30.0.
- **Rendering Backend**: `glow` (OpenGL / DirectX on Windows) with `winit` event loop.
- **Font Pipeline**: Vector typography rendered through `egui` font engine with custom sizing and DPI scale awareness.
- **Styling Paradigm**: No DOM and no CSS files. Visual presentation is driven programmatically by `egui::Visuals`, `egui::Frame`, `egui::epaint::Stroke`, `egui::epaint::Rounding`, `egui::epaint::Shadow`, and raw canvas drawing via `egui::Painter`.

---

## 4. DETAILED UI/UX ANATOMY & SCREEN INVENTORY

The interface is structured into four primary layout zones, plus floating overlays and modal dialogs:

```
+------------------------------------------------------------------------------------------------------+
| [TopBar] (Optional Breadcrumb / Global Status / Search Bar / Command Palette Trigger)                |
+----------------------+-------------------------------------------------------+-----------------------+
| [Left Sidebar]       | [Main Content Area]                                   | [Right Inspector]     |
| Width: 240px         | Width: Dynamic Flexible                               | Width: 330px          |
| Collapsible (Ctrl+B) |                                                       | Toggleable (Ctrl+Sh+B)|
|                      | [Tab Bar: Sessions, Panes, Open Files]                |                       |
| - Projects Header [+]|-------------------------------------------------------| - Tab: Info           |
|   * Project List     | [Terminal Grid / Split Panes / In-Terminal Editor]   | - Tab: Files (Tree)   |
| - Sessions           |                                                       | - Tab: Git (Status)   |
| - Agents             |                                                       | - Tab: Rig (Board)    |
| - Processes          |                                                       |                       |
| - Git Status         |                                                       |                       |
+----------------------+-------------------------------------------------------+-----------------------+
| [Status Bar] Height: 24px (Path | Active Session | Row:Col | Git Branch | CPU/RAM Stats)             |
+------------------------------------------------------------------------------------------------------+
```

### 4.1. Left Sidebar (Navigation & Resource Manager)
- **Dimensions**: Fixed 240px default width; collapsible via `Ctrl+B` or chevron toggle.
- **Components**:
  1. **Workspace Header**: Displays workspace title and action buttons.
  2. **Projects Section**:
     - Lists all registered projects with active check indicator.
     - Plus (+) button triggers a modern popover menu with 4 explicit choices:
       - `New Session (Ctrl+T)`: Spawns shell in active project.
       - `New Project...`: Opens modal dialog with path input and native Windows folder picker.
       - `New Agent... (Ctrl+Shift+A)`: Opens modal to select agent (`agy`, `claude`, etc.) and optional git worktree.
       - `Open Project... (Ctrl+O)`: Invokes native Windows directory picker.
  3. **Sessions Section**:
     - Tree list of sessions belonging to the active project.
     - Active session highlighted with distinct border and background wash.
  4. **Agents Section**:
     - Real-time list of AI agent sessions.
     - Status dot marking agent execution state: green for ready, yellow for working/thinking, red for error, grey for queued.
  5. **Processes Section**:
     - Lists supervised background background tasks (e.g., `web`, `dev`, `watch`).
     - Port badges (e.g., `:3000`, `:8080`) indicating open TCP listeners.
  6. **Git Status Widget**:
     - Displays current checked-out branch name.
     - Indicators for added (`+N`) and modified/deleted (`-N`) files.

### 4.2. Main Content Area (Terminal & Editor Hub)
- **Tab Bar**:
  - Horizontal tab row displaying open terminal sessions and open code editor files.
  - Close button (`x`) per tab.
  - Active tab rendered with elevated card treatment and high-contrast outline.
- **Terminal Panes (Split Layout)**:
  - Supports horizontal and vertical splits (`Ctrl+Shift+D` / `Ctrl+Shift+R`).
  - Pane Zoom / Maximize mode (`Ctrl+Shift+Z` or `Ctrl+Shift+Enter`) to focus on a single pane.
  - Close Pane (`Ctrl+W`) closes only the focused pane, keeping remaining splits alive.
  - Full VT100 / ANSI color sequence rendering (standard 16 colors, 256 colors, and RGB true color).
  - Highlighting & Selection: Drag mouse selection, copy-on-select toggle, right-click paste.
  - Keyboard signal handling: Full `Ctrl+C` interrupt transmission to Windows ConPTY child processes.
- **Built-in Code Editor & Diff Viewer**:
  - Embedded lightweight code editor matching terminal session context.
  - Syntax highlighting for Rust, Go, JavaScript, TypeScript, Python, HTML, CSS, JSON, TOML, Markdown.
  - In-place Diff View (`F7`) comparing working tree disk content with in-memory buffer.
  - Dirty state indicator (`*`) and unsaved changes confirmation modal upon close (`Ctrl+W` / `F11`).

### 4.3. Right Inspector Panel (Context & Telemetry)
- **Dimensions**: Fixed 330px default width; toggleable via `Ctrl+Shift+B`.
- **Four Integrated Tabs**:
  1. **Info Tab (`Ctrl+Shift+I`)**:
     - System Telemetry: CPU percentage, memory utilization, uptime.
     - Active Project metadata and root directory path.
     - **Running Shells List**: Enumerates every active shell pane across all sessions with active status dot, pane name, session name, and click-to-switch navigation.
  2. **Files Tab (`Ctrl+Shift+E`)**:
     - Hierarchical file explorer tree.
     - Expandable directory nodes with folder and file type vector icons.
     - Single-click opens file in editor; context menu for new file creation.
  3. **Git Tab (`Ctrl+Shift+G`)**:
     - Visual branch indicator and remote tracking status.
     - Staged and unstaged files breakdown.
     - Quick commit and diff inspection.
  4. **Rig Tab**:
     - Rig Board integration showing AI swarm tasks and coordination cards.
     - Visual state columns: Needs you, Working, Ready, Queued, Done.

### 4.4. Bottom Status Bar
- **Dimensions**: Fixed 24px height.
- **Information Items**:
  - Left: Active project name, relative folder path.
  - Center: Terminal cursor position (`Ln X, Col Y`), active shell process name.
  - Right: Git branch name with branch icon, running process counter, CPU utilization pill (`CPU 26%`).

### 4.5. Overlays and Modal Dialogs
1. **Command Palette (`Ctrl+Shift+P` / `F2`)**:
   - Floating centered palette with fuzzy search input.
   - Categorized actions: Terminal, View, Project, Agent, Editor.
   - Keyboard shortcut badges on the right side of list items.
2. **Quick Open File Picker (`Ctrl+P`)**:
   - High-speed file search across the workspace.
   - Instant keyboard navigation (Up/Down arrows, Enter to open).
3. **Settings Modal (`Ctrl+,`)**:
   - Comprehensive configuration modal organized into 4 semantic groups and 11 categories:
     - Application: General, Appearance.
     - Workspace: Terminal, Editor, Browser, Keyboard.
     - Integrations: Notifications, Agents, Web Access, Dev URLs.
     - System: About.
   - Real-time persistent state management synced to `settings.json`.
4. **Keyboard Shortcuts Cheatsheet (`F1`)**:
   - Complete grid modal displaying categorized shortcut combinations.
5. **New Agent Dialog (`Ctrl+Shift+A` / `F4`)**:
   - Agent selector dropdown (`agy`, `claude`, `codex`, `opencode`, `pi`, `shell`).
   - Worktree configuration checkboxes (isolated branch vs current tree).
   - "Start Agent" button immediately executes agent process in session and closes dialog.
   - "Cancel" button cleanly dismisses dialog without side effects.
6. **Add Process Dialog (`Ctrl+Shift+N` / `F6`)**:
   - Form inputs for process name, command, working directory, port, autostart toggle, restart policy, file watch globs, and environment variables.
7. **New Project & Open Project Dialogs**:
   - Folder path text input with validation and "Browse..." button invoking native Windows folder selection.

---

## 5. CURRENT DESIGN TOKENS & VISUAL SPECIFICATION

The current visual system is defined in `crates/temp-terminal-ui/src/workspace.rs` under `pub mod theme`:

### 5.1. Surface Color Palette
```rust
// Canvas and background surfaces
pub const WASH: Color32          = Color32::from_rgb(0x11, 0x16, 0x1c); // #11161c - Main background wash
pub const CARD: Color32          = Color32::from_rgb(0x15, 0x1b, 0x23); // #151b23 - Terminal pane and card base
pub const CARD_RAISED: Color32   = Color32::from_rgb(0x1c, 0x24, 0x30); // #1c2430 - Raised card / Active tab
pub const POPOVER: Color32       = Color32::from_rgb(0x1f, 0x27, 0x35); // #1f2735 - Modal dialogs and floating menus
```

### 5.2. Interactive State Colors
```rust
// Hover and Selection
pub const HOVER: Color32         = Color32::from_rgb(0x22, 0x2c, 0x3d); // #222c3d - Elevated slate hover
pub const SELECTED: Color32      = Color32::from_rgb(0x1d, 0x3a, 0x5f); // #1d3a5f - Active item selection
pub const BORDER: Color32        = Color32::from_rgb(0x28, 0x35, 0x47); // #283547 - Subtle boundary stroke
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x38, 0x4a, 0x61); // #384a61 - Emphasized border stroke
pub const DIVIDER: Color32       = Color32::from_rgb(0x1e, 0x26, 0x33); // #1e2633 - Structural divider line
```

### 5.3. Semantic Status & Accent Colors
```rust
// Accent
pub const ACCENT: Color32        = Color32::from_rgb(0x2f, 0x81, 0xf7); // #2f81f7 - Brand blue accent
pub const ACCENT_HOVER: Color32  = Color32::from_rgb(0x4c, 0x93, 0xf8); // #4c93f8 - Lightened accent hover

// Status Indicators
pub const OK: Color32            = Color32::from_rgb(0x63, 0xd5, 0x8d); // #63d58d - Success / Ready
pub const WARN: Color32          = Color32::from_rgb(0xe9, 0xc6, 0x6d); // #e9c66d - Warning / In Progress
pub const BAD: Color32           = Color32::from_rgb(0xff, 0x6f, 0x6f); // #ff6f6f - Error / Terminated
pub const INFO: Color32          = Color32::from_rgb(0x5a, 0xa9, 0xff); // #5aa9ff - Information
```

### 5.4. Typography Colors
```rust
pub const TEXT_STRONG: Color32   = Color32::from_rgb(0xff, 0xff, 0xff); // #ffffff - Pure white headings
pub const TEXT: Color32          = Color32::from_rgb(0xd7, 0xdd, 0xe5); // #d7dde5 - Primary body text
pub const TEXT_SOFT: Color32     = Color32::from_rgb(0xab, 0xb5, 0xc4); // #abb5c4 - Secondary descriptions
pub const TEXT_MUTED: Color32    = Color32::from_rgb(0x7f, 0x8a, 0x99); // #7f8a99 - Tertiary labels
pub const TEXT_FAINT: Color32    = Color32::from_rgb(0x57, 0x62, 0x73); // #576273 - Inactive or disabled text
```

### 5.5. Geometry and Rounding Tokens
- Window Modal Rounding: `10.0px`
- Popover Menu Rounding: `6.0px`
- Interactive Widget Rounding: `6.0px`
- Stroke Width: Default `1.0px` for borders, `1.5px` for active outlines.

---

## 6. VECTOR ICONOGRAPHY SYSTEM

Rather than relying on external image bitmaps or fragile icon fonts, `temp-terminal` features an integrated vector icon drawing suite in `crates/temp-terminal-ui/src/icons.rs`. Each icon is drawn procedurally using `egui::Painter` line segments, rectangles, and arcs.

### Inventory of Available Vector Icons:
1. `draw_logo`: Glowing terminal shield with TT monogram.
2. `draw_terminal`: CLI angle prompt `>` with trailing cursor line.
3. `draw_folder`, `draw_folder_plus`, `draw_folder_git`: Hierarchical folder tabs with git badges.
4. `draw_git_branch`: Git branch node and divergence fork.
5. `draw_bot`: AI robot head with antenna and eye slits.
6. `draw_check`, `draw_alert`, `draw_spinner`: Status marks and animated rotational spinner.
7. `draw_search`: Magnifying glass with angled handle.
8. `draw_split_right`, `draw_split_down`: Terminal split pane glyphs.
9. `draw_maximize`, `draw_close`, `draw_plus`: Window management and creation actions.
10. `draw_gear`, `draw_globe`, `draw_keyboard`: System settings and shortcut glyphs.
11. `draw_panel_left_close`, `draw_panel_right`: Sidebar and inspector collapse toggles.
12. `draw_chevron_right`, `draw_chevron_down`: Collapsible tree disclosures.
13. `draw_file_text`, `draw_file_plus`, `draw_image`: File type glyphs.
14. `draw_radio`, `draw_tray_arrow_down`, `draw_stash_box`: Git and process action glyphs.

---

## 7. COMPLETE KEYBOARD SHORTCUT DISPATCH TABLE

The application is engineered for complete keyboard-driven navigation:

| Key Binding | Target Action | Scope / Context |
|---|---|---|
| `Ctrl+T` | New Session | Active Project |
| `Ctrl+W` | Close focused split pane / active editor tab | Main Workspace |
| `Ctrl+O` | Open Project (invokes native picker) | Global |
| `Ctrl+P` | Toggle Quick Open file finder | Global Overlay |
| `Ctrl+Shift+P` / `F2` | Toggle Command Palette | Global Overlay |
| `Ctrl+,` | Toggle Settings Modal | Global Overlay |
| `Ctrl+B` | Toggle Left Sidebar | Global |
| `Ctrl+Shift+B` | Toggle Right Inspector | Global |
| `Ctrl+Shift+D` | Split Terminal Down (Vertical) | Active Terminal Session |
| `Ctrl+Shift+R` | Split Terminal Right (Horizontal) | Active Terminal Session |
| `Ctrl+Shift+Z` / `Ctrl+Shift+Enter` | Toggle Maximize / Zoom focused pane | Active Terminal Session |
| `Ctrl+Shift+A` / `F4` | Toggle New Agent Modal | Global Overlay |
| `Ctrl+Shift+N` / `F6` | Toggle Add Process Modal | Global Overlay |
| `Ctrl+Shift+I` | Switch Inspector to Info Tab | Inspector Panel |
| `Ctrl+Shift+E` | Switch Inspector to Files Tab | Inspector Panel |
| `Ctrl+Shift+G` | Switch Inspector to Git Tab | Inspector Panel |
| `F1` | Toggle Keyboard Shortcuts Modal | Global Overlay |
| `F7` | Toggle In-Terminal Diff View | Active Editor Buffer |
| `F11` | Close active editor file | Active Editor Buffer |
| `F12` / `Ctrl+S` | Save active editor file to disk | Active Editor Buffer |
| `Escape` | Dismiss topmost modal, popover, or overlay | Global Stack |

---

## 8. CAPABILITIES, BOUNDARIES & `egui` CONSTRAINTS

When crafting redesign specifications for `temp-terminal`, the following technological characteristics of `egui` must be understood:

### What `egui` Can Do Exceptionally Well:
- **Instantaneous Canvas Manipulation**: Any 2D shape, polygon, gradient, or spline can be rendered via `ui.painter().add(...)`.
- **Custom Box Shadows**: `egui::epaint::Shadow` allows rich, multi-layered soft drop shadows around cards and floating dialogs.
- **Dynamic Corner Rounding**: `Rounding { nw, ne, sw, se }` allows asymmetric corner radii (e.g., rounded top tabs with flat bottoms).
- **Smooth High-FPS Animations**: `ctx.animate_bool(id, open)` and `ctx.animate_value_with_time(id, target, duration)` calculate frame-rate independent interpolation (lerp) for hover effects, collapse animations, and progress fills at 120+ FPS.
- **Theme Swapping**: Modifying `ctx.set_visuals(visuals)` instantaneously repaints the entire window in a single frame without reloading or re-parsing styles.

### Differences from Web Browser / DOM / CSS:
- **No Cascading Style Sheets (CSS)**: There are no `.css` files, classes, or selectors. All styling is expressed through Rust structs (`Frame`, `Stroke`, `Margin`, `Color32`).
- **No Native CSS Flexbox/Grid Engine**: Layout is constructed using immediate-mode primitives: `ui.horizontal(|ui| ...)`, `ui.vertical(|ui| ...)`, `ui.columns(n, |columns| ...)`, or explicit rectangular allocation `ui.allocate_rect(rect, sense)`.
- **Immediate-Mode Execution**: The UI function runs 60 to 120 times per second. State must not be kept inside local GUI widgets; it must reside in Rust struct state models (`WorkspaceApp`).

---

## 9. REDESIGN OBJECTIVES & SCOPE FOR FRONTIER AI MODELS

The current interface is functionally sound, reliable, and high-performance. However, the visual aesthetics currently adhere to an early engineering baseline.

### Primary Goals for the Visual Redesign:
1. **Modern High-Craft Aesthetic**: Elevate the UI to match the visual sophistication of modern flagship developer tools such as **Linear**, **Raycast**, **Warp Terminal**, and **Zed Editor**.
2. **Cohesive Color Hierarchy**:
   - Establish a refined palette with distinct surface layers: canvas base, panel surface, card surface, raised elevation, and popover glass.
   - Introduce subtle, intentional accent treatments (e.g., deep indigo, electric cyan, or refined violet) without visual noise.
3. **Refined Component Ergonomics**:
   - Redesign tab headers into modern pill-style indicators or clean active-underline tabs.
   - Modernize the Left Sidebar with polished collapsible groups, badge counters, and crisp icon-label alignment.
   - Transform the Right Inspector into an executive developer dashboard with visual metric cards and interactive tree items.
   - Polish modal dialogs with smooth floating backdrop scrims and elegant input fields.
4. **Enhanced Micro-Interactions**:
   - State transitions for button hover, active click depressions, and focus rings.
   - Smooth sidebar toggle collapse transitions.
   - Animated status indicators (subtle breathing or pulsing for running agents/processes).
5. **Accessibility & WCAG AA Compliance**:
   - Maintain minimum 4.5:1 text contrast ratios against background surfaces.
   - Ensure clear 3:1 non-text boundary contrast for interactive elements.

---

## 10. SYSTEM PROMPT TEMPLATE FOR FRONTIER AI REDESIGN

When utilizing this specification in another advanced AI model, use the following prompt template:

```markdown
You are a Principal Design Systems Architect and Senior Rust GUI Engineer specializing in `egui`, `eframe`, and modern developer workspace UI/UX (Linear, Raycast, Warp, Zed).

Refer to the provided SYSTEM UI/UX SPECIFICATION & ARCHITECTURE DOSSIER for `temp-terminal`.

Your task is to generate:
1. A Complete Design System Specification (Design Tokens, Color Palettes, Typography Scales, Corner Radii, Elevation Shadows, and Component States).
2. Screen-by-Screen Component Redesign Blueprints (Left Sidebar, Main Terminal Area, Tab Bar, Right Inspector, Status Bar, and Modals).
3. Production-Ready Rust `egui` Implementation Code snippets for:
   - The global `theme` module with exact `Color32`, `Frame`, and `Stroke` definitions.
   - The modernized `render_sidebar` implementation.
   - The modernized `render_tabbar` and split pane layout.
   - The modernized `CommandPaletteState` and modal overlay renderer.
   
Constraints:
- Must use native Rust 2021 and `egui` 0.30 primitives.
- Zero DOM / CSS assumptions. Everything must be executable in `temp-terminal-ui`.
- High-contrast, accessible WCAG AA compliance.
- No decorative emojis in code. Professional developer aesthetic.
```

---
*End of Specification Dossier.*
