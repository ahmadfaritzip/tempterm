# TEMP-TERMINAL UI/UX REDESIGN SPECIFICATION
## Phase 1: Research, Design System & Comprehensive Proposal

**Product:** `temp-terminal` (Modern Agent Workspace & Headless Process Supervisor)  
**Document Version:** `v1.0.0-draft`  
**Date:** 2026-11-08  
**Status:** PROPOSED (Awaiting User Review & Approval Before Implementation)  
**Target Platform:** Windows x64 (`egui` / `eframe` Desktop GUI)  

---

## 1. Executive Summary & Vision

`temp-terminal` is a modern, high-performance developer workspace and autonomous agent supervisor written in pure Rust. While the engine, ConPTY integration, split layout, process supervisor, editor, and diff viewer achieve 100% functional parity with the original Go application (`omaterm`), the visual design currently suffers from the aesthetic limitations of its Go/Gio ancestor.

The original Go UI relied on a flat, low-contrast, monochromatic `#0c0f14` palette with rudimentary pill tabs, inconsistent unicode glyphs, cramped modal dialogues, and an uninspiring layout.

This redesign proposal establishes a **world-class, modern, pro-developer visual identity** code-named **"Linear / Obsidian Slate"**. The new design transforms `temp-terminal` into an elegant, high-productivity environment comparable to modern developer tools like Linear, Raycast, Warp, and Zed, while strictly preserving:
- **100% of existing features and workflows.**
- **All existing keyboard shortcuts and global input dispatchers.**
- **Headless core, daemon IPC, process supervisor, and ConPTY streaming logic.**
- **Zero modification to the backend crates or Go baseline.**

---

## 2. Current UI/UX Weaknesses & Friction Points

An exhaustive audit of the current Rust `temp-terminal-ui` implementation revealed the following architectural and visual deficiencies:

| Component | Current Implementation | Identified Weakness / Friction Point |
|---|---|---|
| **Visual Hierarchy & Depth** | Entire UI uses uniform `#0c0f14` with flat 1px faint border strokes. | Total lack of surface elevation. Sidebar, topbar, canvas, and panels blend into an undifferentiated sea of dark rectangles. |
| **Tab Bar & Navigation** | Round pill buttons (`rounding: 13.0`) with colored status dots. | Look like mobile chips from 2018. Pill shapes lack tab structure, waste vertical space, and lack a clear active bottom-border highlight. |
| **Topbar Layout** | Mixed horizontal strip of breadcrumbs, session pills, and action buttons (`+ New`, Split, Inspector, Settings). | Cluttered and unorganized. Actions have no visual weight separation. Dropdown menus use plain text frames. |
| **Sidebar Hierarchy** | Sections titled "PROJECTS", "SESSIONS", "PROCESSES", "DEVURLS" with loose lists. | High visual noise. Unicode glyphs (`📁+`, `◀`, `✕`) render inconsistently across Windows fonts. Missing collapsed state icons. |
| **Pane Headers** | Minimal 24px header with raw title and `⛶` button. | Hard to tell which terminal pane has active keyboard focus when borders are faint. Lacks process status telemetry (e.g. running/stopped, exit code). |
| **Split Dividers** | 8px divider with subtle gray pill grip. | Grip is functional but visually basic. Hover states lack snappy visual cues. |
| **Dialogs & Modals** | Basic vertical forms inside standard `egui::Window` frames. | Settings modal presents 11 tabs as a wrapped horizontal button bar, making navigation cumbersome. Add Process/New Agent dialogs lack structured input grouping. |
| **Empty Workspace State** | 3 tall vertical cards in a centered column. | Basic borders, plain text descriptions. Misses the opportunity to showcase keyboard shortcuts and quick-start actions elegantly. |
| **Iconography** | Inconsistent unicode symbols (`❖`, `⚙`, `📁`, `◫`, `▶`, `✓`, `✕`). | Dependent on OS font fallback; some appear as outline symbols, others as colored emojis, creating an unpolished visual tone. |
| **Status Bar** | Simple 24px bottom strip with pipe-delimited text (`git:main \| cwd \| active:1`). | Monotone; lacks badge chips for git branch, process status, and zoom indicator. |

---

## 3. Product Purpose & User Personas

`temp-terminal` serves three primary technical roles:

1. **Autonomous AI Agent Supervision:**
   - Developers run agents (Claude Code, OpenAI Codex, Gemini CLI, gh-copilot) inside terminal panes.
   - Users need immediate visibility into agent status (thinking, running tool, awaiting input), turn history, and file modifications.
2. **Multi-Terminal Workspace & Multiplexing:**
   - Power users split panes vertically and horizontally across multiple active shells (PowerShell, CMD, WSL bash).
   - Fast keyboard navigation, instant zoom (`Ctrl+Shift+Z`), and zero-latency input are non-negotiable.
3. **Local Dev Server & Process Supervision:**
   - Developers define long-running processes (`omaterm.yml`) with TCP readiness checks, automatic reload on file changes, and restart backoffs.
   - Status indicators must clearly convey health states (starting, healthy/listening, restarting, failed).

---

## 4. Exploration of Design Directions

Three distinct design directions were evaluated:

### Direction A: "Obsidian / Linear Slate" (Recommended)
- **Concept:** Ultra-clean, pro-grade minimalist studio inspired by Linear, Raycast, and Zed.
- **Color:** Deep matte obsidian base (`#0b0d11`), structured charcoal panels (`#12151b`), razor-sharp 1px subtle borders (`rgba(255,255,255,0.08)`), with electric sapphire (`#3b82f6`) or vibrant violet (`#8b5cf6`) accents.
- **Surfaces:** Explicit 4-layer elevation hierarchy (L0 Canvas -> L1 Structural Panels -> L2 Cards/Panes -> L3 Floating Modals).
- **Tabs:** Crisp rectangular tabs with 4px corner rounding, active underline indicator, and close button on hover.
- **Why it wins:** Highest legibility, professional authority, zero gimmickry, zero cognitive fatigue over 12-hour coding days, and perfectly suited to `egui`'s immediate-mode rendering strengths.

### Direction B: "Cyber-Terminal Neo"
- **Concept:** Futuristic mission-control deck with carbon textures, cyber emerald (`#10b981`) or high-visibility amber (`#f59e0b`) accents, and monospace telemetry.
- **Trade-offs:** Visually exciting for short sessions, but high-contrast neon borders cause eye strain during extended work.

### Direction C: "Nordic Craft / Warm Charcoal"
- **Concept:** Warm dark palette inspired by JetBrains Fleet and Tokyo Night (`#16161e`), soft pastel accents, rounded 8–10px corners.
- **Trade-offs:** Softer and friendly, but lower information density and less authoritative as a developer terminal tool.

---

## 5. Design System: "Obsidian / Linear Slate"

### 5.1 Color Palette & Semantic Tokens

```
=============================================================================
TOKEN                  HEX / RGBA                  SEMANTIC USAGE
=============================================================================
SURFACES & BACKGROUNDS:
  SURFACE_CANVAS       #090b0e (RGB 9, 11, 14)     Root app background / viewport underlay
  SURFACE_PANEL        #0e1117 (RGB 14, 17, 23)    Sidebar, topbar, status bar, inspector
  SURFACE_CARD         #141820 (RGB 20, 24, 32)    Terminal pane background, editor canvas
  SURFACE_ELEVATED     #1c212b (RGB 28, 33, 43)    Active tabs, raised cards, dropdown menus
  SURFACE_INPUT        #11141b (RGB 17, 20, 27)    Text inputs, search boxes, code frames
  SURFACE_OVERLAY      #181c25 (RGB 24, 28, 37)    Modal dialog frames, command palettes
  SURFACE_HOVER        rgba(255,255,255, 0.04)     Generic interactive item hover tint
  SURFACE_ACTIVE       rgba(255,255,255, 0.08)     Pressed item feedback

BORDERS & DIVIDERS:
  BORDER_SUBTLE        rgba(255,255,255, 0.06)     Subtle card edges, structural dividers
  BORDER_MEDIUM        rgba(255,255,255, 0.12)     Active pane headers, card boundaries
  BORDER_FOCUS         #3b82f6 (RGB 59, 130, 246)  Focused terminal pane, active input edge
  DIVIDER_LINE         rgba(255,255,255, 0.08)     Separator between panels and sections

TYPOGRAPHY & TEXT:
  TEXT_PRIMARY         #f0f3f6 (RGB 240, 243, 246) Headings, active tabs, primary commands
  TEXT_SECONDARY       #c2c9d2 (RGB 194, 201, 210) Body text, editor code, regular labels
  TEXT_MUTED           #768390 (RGB 118, 131, 144) Subtitles, secondary metadata, shortcuts
  TEXT_DISABLED        #48525d (RGB 72, 82, 93)    Inactive elements, line numbers gutter

BRAND ACCENT & INTERACTION:
  ACCENT_PRIMARY       #3b82f6 (RGB 59, 130, 246)  Primary brand blue, active focus indicator
  ACCENT_HOVER         #60a5fa (RGB 96, 165, 250)  Hover state on primary buttons
  ACCENT_SUBTLE        rgba(59, 130, 246, 0.15)    Selected row background, tab active glow
  ACCENT_MUTED         rgba(59, 130, 246, 0.30)    Subtle accent borders

SEMANTIC STATUS:
  STATUS_SUCCESS       #22c55e (RGB 34, 197, 94)   Listening port, running process, tests pass
  STATUS_WARNING       #f59e0b (RGB 245, 158, 11)  Process restarting, unsaved changes, dirty
  STATUS_ERROR         #ef4444 (RGB 239, 68, 68)   Failed process, conflict, syntax error
  STATUS_INFO          #38bdf8 (RGB 56, 189, 248)  Agent turn, git head, informatory notices
=============================================================================
```

### 5.2 Elevation & Layer Hierarchy

Visual depth is achieved through **4 strict elevation planes**:

```
Layer 3: Floating Overlays [Modals, Command Palette, Dropdowns, Tooltips]
         Fill: SURFACE_OVERLAY (#181c25) | Stroke: BORDER_MEDIUM | Shadow: 12px blur
   ▲
Layer 2: Content Cards & Workspace Panes [Terminal Panes, Editor, Diff Columns]
         Fill: SURFACE_CARD (#141820) | Stroke: BORDER_SUBTLE | Radius: 6.0px
   ▲
Layer 1: Structural Navigation Chrome [Top Bar, Sidebar, Inspector, Status Bar]
         Fill: SURFACE_PANEL (#0e1117) | Stroke: BORDER_SUBTLE | Radius: 0px
   ▲
Layer 0: Canvas Base [Root Application Window Viewport]
         Fill: SURFACE_CANVAS (#090b0e)
```

### 5.3 Typography Scale & Font Hierarchy

- **UI Font Family:** Segoe UI / Inter (Proportional).
- **Code & Terminal Font Family:** Cascadia Code / Consolas (Monospace).

| Level | Size | Weight | Usage |
|---|---|---|---|
| **Display / Modal Title** | 16.0 px | Bold (`700`) | Dialog headers, empty state title |
| **Section Header** | 11.0 px | Semi-Bold (`600`) | Sidebar group titles ("PROJECTS", "PROCESSES") |
| **Primary UI / Tab** | 13.0 px | Medium (`500`) | Tab titles, button labels, breadcrumbs |
| **Secondary / Meta** | 11.5 px | Regular (`400`) | Status bar, process ports, file paths |
| **Micro / Badge** | 10.0 px | Semi-Bold (`600`) | Keyboard shortcut chips (`Ctrl+B`), count badges |
| **Terminal / Code** | 12.0 px | Regular (`400`) | Terminal cells, editor buffer, diff view |

### 5.4 Shape & Corner Radii

- **Sharp Micro Elements (Inputs, Buttons, Tab Pills):** `4.0px`
- **Container Cards (Terminal Panes, Process Cards, Action Cards):** `6.0px`
- **Modal Dialog Frames & Large Overlays:** `8.0px`
- **Divider Pill Grip:** `3.0px` (smooth capsule)

---

## 6. Component-by-Component Redesign Specifications

### 6.1 Top Bar & Tab Navigation
- **Height:** 42 px (streamlined and proportioned).
- **Background:** `SURFACE_PANEL` with bottom border `BORDER_SUBTLE`.
- **Left Section:**
  - App Logo icon + breadcrumb: `temp-terminal / <Active Project>`.
  - Sidebar toggle button with clean vector icon.
- **Center Section (Tabs):**
  - Modern rectangular segmented tabs instead of oval pills.
  - Active Tab: `SURFACE_ELEVATED` background, 2px bottom accent bar (`ACCENT_PRIMARY`), white text, and a crisp hoverable `×` close button.
  - Inactive Tabs: Transparent background, muted text, subtle hover highlight (`SURFACE_HOVER`).
  - Active Editor Tabs: Integrated alongside terminal sessions with file icon and dirty indicator `●`.
- **Right Section (Actions):**
  - Grouped action cluster with dedicated button styling:
    - `[+ New ▾]` primary split button.
    - `[Split ◫]` with dropdown or direct vertical/horizontal split.
    - `[Inspector ⎘]` with toggle active state.
    - `[Settings ⚙]` button.

### 6.2 Left Sidebar (Projects, Sessions, Processes, DevURLs)
- **Width:** 240 px default (resizable 200–480 px).
- **Collapsible:** Smooth collapse with floating expand button.
- **Section Headers:**
  - Compact 20px headers with collapsible chevron (`▾` / `▸`), section count badge (e.g., `SESSIONS 2`), and inline action button (`+`).
- **Project Switcher:**
  - Active project rendered as a distinct elevated card with folder icon, current git branch badge, and switch dropdown.
- **Session List:**
  - Each session item has an active indicator bar on the left edge, session name, process count, and close button on hover.
- **Process Cards:**
  - Redesigned status badge: solid dot (Green = Running, Amber = Restarting, Red = Failed) with clear status label, port badge (e.g., `:3000`), and quick-restart button.

### 6.3 Terminal Panes & Split Workspace
- **Pane Frame:**
  - Clean card container with `SURFACE_CARD` background and `6.0px` rounding.
  - **Active Focused Pane:** Highlit by `1.5px` border in `BORDER_FOCUS` (`#3b82f6`) and a glowing subtle accent bar on the pane header.
  - **Inactive Panes:** Subtle `BORDER_SUBTLE` stroke.
- **Pane Header:**
  - Clean 26px header strip with:
    - Left: Shell icon (PowerShell / CMD), title, PID badge.
    - Right: Zoom toggle `[⛶]` and Close `[×]` buttons.
- **Split Dividers:**
  - 8px hit area with 1px center hairline `BORDER_SUBTLE`.
  - Centered capsule grip handle (`30px × 4px`) that lights up in `ACCENT_PRIMARY` on hover or drag.
  - Double-click resets split to equal 50/50 ratio.

### 6.4 Right Inspector Panel (Agent Telemetry & Files)
- **Header:** Tabbed selector between `[Agent Inspector]` and `[Modified Files]`.
- **Agent Inspector Tab:**
  - Agent status hero card: `Claude Code` or `Codex` with active state badge (`THINKING`, `RUNNING_TOOL`, `IDLE`).
  - Turn timeline: Visual vertical step list of agent turns with elapsed duration and exit status.
  - Inter-Agent Mailbox messages displayed in styled message cards.
- **Modified Files Tab:**
  - Tree/list of files modified in the active workspace.
  - Clicking a file opens the built-in diff viewer instantly.

### 6.5 In-App Editor & Markdown Live Preview
- **Editor Canvas:**
  - Dark mode syntax highlighting using pro theme tokens.
  - Line numbers gutter cleanly separated with subtle vertical divider.
  - Line number of current caret highlighted in `TEXT_PRIMARY`.
  - Soft word wrap toggle and status coordinate pill (`Ln 42, Col 18`) in bottom right.
- **Markdown Live Preview:**
  - Centered 720 DIP max-width document layout.
  - Headings styled with proportional font scales, subtle divider lines under H1/H2.
  - Code blocks framed in `SURFACE_INPUT` with language tag chip.
  - Callout blockquotes with vertical accent bar.
  - Toggle between `[Source]` and `[Preview]` with smooth state transition.

### 6.6 Diff Viewer & Git Staging Toolbar
- **Header Toolbar:**
  - File path breadcrumb, total additions (`+14` green chip), deletions (`-3` red chip).
  - Segmented control for `[Split (Side-by-Side)]` vs `[Inline (Unified)]`.
  - Git action button group:
    - `[Stage File]` (primary accent button).
    - `[Unstage File]` (secondary button).
    - `[Discard Changes]` with two-step confirmation (`Discard? [Confirm] [Cancel]`).
- **Diff Viewport:**
  - Gutter with line numbers for old and new versions.
  - Additions highlighted with `rgba(34, 197, 94, 0.15)` background and `+` marker.
  - Deletions highlighted with `rgba(239, 68, 68, 0.15)` background and `-` marker.

### 6.7 Dialogs & Modals (Settings, Command Palette, Forms)
- **Backdrop Dimming:**
  - Modals render with a subtle 40% black backdrop overlay to focus attention and mask background terminal activity.
- **Command Palette (`Ctrl+P` / `Ctrl+K`):**
  - Centered floating modal (600px width).
  - Search input with search icon and live filter list.
  - Shortcut hints on right (`↵ to run`, `Esc to dismiss`).
- **Settings Modal (`Ctrl+,`):**
  - Redesigned into a **Two-Column Master-Detail Layout**:
    - Left Column (180px): Vertical navigation list of 11 categories (General, Terminal, Editor, Processes, Agents, DevURLs, Keybindings, Theme, Network, Advanced, About).
    - Right Column: Clean, spacious settings panel with organized sections, toggle switches, numeric sliders, and dropdown selectors.
- **Add Process & New Agent Modals:**
  - Structured fieldsets with clear input labels, helper text, and validation error callouts.

### 6.8 Status Bar
- **Height:** 24 px.
- **Left:** Git branch badge (` main`), current working directory path, daemon status indicator (`● Daemon: Connected`).
- **Center:** Status messages or active agent status banner.
- **Right:** Zoom indicator chip (when active), terminal dimensions (`80×24`), and quick shortcut hints (`F1 Help`).

### 6.9 Empty State Hero
- Centered container with clean typography:
  - App icon + *"Welcome to temp-terminal"*.
  - Subtitle: *"Fast, headless-first agent workspace & process supervisor"*.
  - 3 polished horizontal cards with icons, descriptions, and keyboard shortcut chips:
    1. `[New Agent Session]` — `F4`
    2. `[Add Process]` — `F6`
    3. `[Open Terminal]` — `Ctrl+T`
  - Recent projects quick-access list below the cards.

---

## 7. Accessibility, Contrast & Micro-Interactions

1. **WCAG AA Compliance:**
   - Text contrast ratios strictly maintained above **4.5:1** for standard text and **3:1** for large text and UI components.
   - `TEXT_PRIMARY` (`#f0f3f6`) on `SURFACE_PANEL` (`#0e1117`) achieves an **18.2:1** contrast ratio.
   - `TEXT_SECONDARY` (`#c2c9d2`) on `SURFACE_PANEL` achieves an **11.5:1** contrast ratio.
2. **Keyboard Focus Rings:**
   - Every interactive control (buttons, inputs, tabs) renders an explicit `1.0px` `BORDER_FOCUS` outline when focused via keyboard navigation.
3. **Cursor Blink & PTY Throttling:**
   - Blinking cursor calibrated to 1400ms cycle (800ms ON / 600ms OFF) with repaint throttling to ensure 0% CPU consumption when idle.

---

## 8. Implementation Strategy & Architecture

### Phase A: Design Tokens & Theme Foundation
- Refactor `temp_terminal_ui::workspace::theme` into a dedicated, clean module with the complete "Linear / Obsidian Slate" token set.
- Implement reusable styling helpers for cards, buttons, badges, and headers.

### Phase B: Structural Chrome Refactoring
- Refactor `render_topbar` and `render_statusbar` with the new segmented tab system, breadcrumbs, and grouped controls.
- Refactor `render_sidebar` into clean collapsible sections with styled cards and badges.

### Phase C: Workspace & Split Panes Polish
- Enhance `render_single_pane` with the new active border highlight, polished pane header, and capsule divider grip.
- Polish `render_empty_state` with the new pro hero cards and keyboard chips.

### Phase D: Dialogs & Overlays Overhaul
- Transform the Settings modal into the two-column master-detail layout.
- Polish Command Palette, Quick Open, New Agent, and Add Process dialogs with the modal backdrop and consistent form fields.

### Phase E: Editor, Markdown & Diff Polish
- Apply refined typography, gutter styling, and preview container padding.
- Polish diff viewer toolbar buttons and status chips.

---

## 9. Non-Negotiable Constraints & Safety Guarantees

- **Zero Backend Modification:** All changes are strictly confined to `temp-terminal/crates/temp-terminal-ui/`.
- **Keyboard Shortcuts Preservation:** All shortcuts (`Ctrl+B`, `Ctrl+T`, `Ctrl+W`, `Ctrl+O`, `Ctrl+P`, `Ctrl+Shift+Z`, `F1`–`F12`) remain 100% functional.
- **Read-Only Repositories:** `omaterm/` and root `RUST REWRITE - TODO.md` remain completely unmodified.
- **Zero Regression:** All 220 existing automated unit and integration tests must continue to pass cleanly.
