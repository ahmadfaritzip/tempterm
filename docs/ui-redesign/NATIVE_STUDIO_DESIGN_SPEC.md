# NATIVE STUDIO DESIGN SPECIFICATION
# Quiet Graphite / Arctic Blue

> Application: `temp-terminal` (Rust Native Developer Workspace)  
> Framework: Rust 2021, egui 0.30, eframe 0.30, Windows ConPTY, Tokio  
> Design Foundation: Native Studio with GPUI Architecture & Styling Principles  
> Design Status: Production Specification  

---

## 1. Design Vision and Philosophy

**Native Studio** is a visual presentation system designed specifically for high-intensity developer workflows. It synthesizes the visual refinement and developer ergonomics of modern tools such as Zed Editor, Linear, Raycast, and Warp Terminal into a cohesive, native Windows desktop application.

### Core Visual Principles

1. **Terminal as Visual Centerpiece**: The terminal canvas is the focal core of the application (#080C11). Surrounding UI surfaces support rather than distract from code and command execution.
2. **Quiet Graphite Foundation**: Surface hierarchy is established through precise value steps (#0C1016 -> #111821 -> #0E141C -> #161F2A -> #1C2734 -> #202D3D -> #1A2C43), eliminating loud borders, harsh dividers, and unnecessary card nesting.
3. **Restrained Arctic Blue Accents**: High-fidelity sapphire/arctic blue (#86A9FF, hover #A0BBFF) is reserved exclusively for primary focus states, active indicators, and explicit primary actions.
4. **Desktop-First and Responsive**: Layout dimensions adapt fluidly to screen real estate across 5 responsive breakpoints, ensuring zero clipped content and optimal space utilization.
5. **One Action, One Primary Location**: Unambiguous action placement eliminates duplicated buttons across toolbars, headers, and menus.
6. **Zero AI Slop / Zero Em Dashes / Zero Decorative Emojis**: Clean, semantic typography with high-contrast text and crisp vector iconography.

---

## 2. Centralized Design Tokens

### Color Palette

| Token | Hex Value | RGB / Alpha | Semantic Role |
|---|---|---|---|
| `APP_BACKGROUND` | `#0C1016` | `(12, 16, 22)` | Global application background / window frame |
| `PANEL_BACKGROUND` | `#111821` | `(17, 24, 33)` | Left sidebar, top navigation, right inspector |
| `WORKSPACE_BACKGROUND` | `#0E141C` | `(14, 20, 28)` | Central workspace canvas background |
| `SURFACE` | `#161F2A` | `(22, 31, 42)` | Cards, input fields, popover containers |
| `SURFACE_ELEVATED` | `#1C2734` | `(28, 39, 52)` | Elevated surfaces, menu items, active list rows |
| `SURFACE_HOVER` | `#202D3D` | `(32, 45, 61)` | Interactive hovered widget backgrounds |
| `SURFACE_SELECTED` | `#1A2C43` | `(26, 44, 67)` | Active tab, focused tree row, selected option |
| `TERMINAL_BACKGROUND` | `#080C11` | `(8, 12, 17)` | Dedicated terminal pane background canvas |
| `BORDER_SUBTLE` | `#253140` | `(37, 49, 64)` | Default 1px hairline boundary divider |
| `BORDER_STRONG` | `#344355` | `(52, 67, 85)` | Elevated boundaries, popover borders, active dividers |
| `TEXT_PRIMARY` | `#E7ECF2` | `(231, 236, 242)` | High-contrast headers, active titles, primary text |
| `TEXT_SECONDARY` | `#A7B2C1` | `(167, 178, 193)` | Body text, section content, tree items |
| `TEXT_MUTED` | `#7D899B` | `(125, 137, 155)` | Descriptions, inactive tabs, secondary metadata |
| `TEXT_DISABLED` | `#606E81` | `(96, 110, 129)` | Disabled text, separators, subtle keycaps |
| `ACCENT` | `#86A9FF` | `(134, 169, 255)` | Primary Arctic Blue accent, active indicators |
| `ACCENT_HOVER` | `#A0BBFF` | `(160, 187, 255)` | Hover state for accent buttons and links |
| `ACCENT_TINT` | `#1A2C43` | `(26, 44, 67)` | Background tint for accent chips and active highlights |
| `SUCCESS` | `#60C995` | `(96, 201, 149)` | Running processes, active agents, git additions |
| `WARNING` | `#E5B566` | `(229, 181, 102)` | Unsaved files, standby processes, git modifications |
| `DANGER` | `#ED777D` | `(237, 119, 125)` | Terminated processes, git deletions, destructive actions |
| `INFO` | `#6DBAFF` | `(109, 186, 255)` | Informational telemetry, session badges, doc previews |

---

## 3. Typography Specification

### Font Prioritization

1. **Proportional UI Font**:
   - Primary: `Inter`
   - Secondary: `Segoe UI`
   - Fallback: System sans-serif

2. **Monospace Code and Terminal Font**:
   - Primary: `Cascadia Code`
   - Secondary: `Cascadia Mono`
   - Fallback: `Consolas`

### Type Scale

| Role | Size | Line Height | Weight | Application |
|---|---|---|---|---|
| `MODAL_TITLE` | 22px | 28px | SemiBold | Modal dialog headers, primary splash titles |
| `PANEL_TITLE` | 16px | 22px | Medium | Sidebar section headers, inspector title |
| `SECTION_HEADING` | 13px | 18px | SemiBold | Group titles, project switcher label |
| `BODY_PRIMARY` | 13px | 18px | Regular | Main content, settings options, file tree |
| `BODY_SECONDARY` | 12px | 16px | Regular | Tab labels, status details, description text |
| `LABEL` | 11px | 14px | Medium | Uppercase section badges, telemetry keys |
| `METADATA` | 10px | 12px | Regular | Timestamps, PID labels, keycaps, footer metrics |
| `TERMINAL_TEXT` | 13.5px | 18px | Monospace | Terminal PTY grid, code editor, diff viewer |

---

## 4. Spacing and Geometry System

### 4px Base Spacing Scale

- `SPACE_MICRO` = 4.0 px: Icon-to-text spacing, compact badge margins, split gap.
- `SPACE_COMPACT` = 8.0 px: Control padding, compact toolbar gaps, list row spacing.
- `SPACE_PADDING` = 12.0 px: Standard container padding, panel inner margins.
- `SPACE_GROUP` = 16.0 px: Component groupings, modal section separators.
- `SPACE_SECTION` = 20.0 px: Large layout section separators.
- `SPACE_MAJOR` = 24.0 px: Major view padding, empty state vertical margins.

### Corner Radii

- `RADIUS_SM` = 4.0 px: Small controls, keycap chips, close buttons.
- `RADIUS_MD` = 6.0 px: Standard buttons, text inputs, list items.
- `RADIUS_CARD` = 8.0 px: Cards, popover menus, floating drawers.
- `RADIUS_MODAL` = 10.0 px: Floating modal dialogs (Command Palette, Settings).
- `RADIUS_PILL` = 12.0 px: Status badges, category filter chips.

---

## 5. Global Layout Architecture

### Standard Layout Dimensions (egui Logical Points)

| Component | Target Dimension | Alignment | Surface Color |
|---|---|---|---|
| Global Top Navigation | 40.0 px height | Full window width | `PANEL_BACKGROUND` (#111821) |
| Workspace Tab Strip | 34.0 px height | Central column top | `WORKSPACE_BACKGROUND` (#0E141C) |
| Left Sidebar | 236.0 px default width | Left side panel | `PANEL_BACKGROUND` (#111821) |
| Right Inspector | 288.0 px default width | Right side panel | `PANEL_BACKGROUND` (#111821) |
| Terminal Pane Header | 30.0 px height | Inside each pane | `TERMINAL_BACKGROUND` (#080C11) |
| Bottom Status Bar | 24.0 px height | Full window width | `APP_BACKGROUND` (#0C1016) |
| Central Workspace | Remaining flex space | Center panel | `WORKSPACE_BACKGROUND` (#0E141C) |

---

## 6. Action Hierarchy and Deduplication

Adhering to **One Action, One Primary Location**:

| Action Scope | Action Name | Primary Location | Access Shortcut |
|---|---|---|---|
| Application | Project Switching | Top Navigation (Left) | Click project button / `Ctrl+O` |
| Application | Command Palette / Search | Top Navigation (Center) | Click trigger / `Ctrl+K` / `F2` |
| Application | Settings Modal | Top Navigation (Right) | Click gear / `Ctrl+,` |
| Application | Toggle Inspector | Top Navigation (Right) | Click panel icon / `Ctrl+Shift+B` |
| Application | New AI Agent | Top Navigation (Right) | Click agent icon / `F4` / `Ctrl+Shift+A` |
| Workspace | New Session / Shell | Workspace Tab Strip | Click (+) button / `Ctrl+T` |
| Workspace | Close Active Tab | Workspace Tab Strip | Click tab (x) / `Ctrl+W` |
| Pane | Split Terminal Right | Active Pane Header | Click split right / `Ctrl+Shift+R` |
| Pane | Split Terminal Down | Active Pane Header | Click split down / `Ctrl+Shift+D` |
| Pane | Maximize / Zoom Pane | Active Pane Header | Click maximize / `Ctrl+Shift+Z` |
| Pane | Close Pane | Active Pane Header | Click pane (x) / `Ctrl+W` |
| Sidebar | Collapse / Expand Sidebar | Sidebar Header / 56px Rail | Click collapse / `Ctrl+B` |
| Sidebar | Add Supervised Process | Sidebar Processes Group | Click (+ Add Process) / `F6` |
| Editor | Save Active File | Editor Header | `Ctrl+S` / `F12` |
| Editor | Toggle Diff View | Editor Header / Tab Strip | `Ctrl+D` / `F7` |

---

## 7. Responsive Design Breakpoints

Responsive layouts adapt dynamically according to viewport logical width:

1. **Large Desktop (>= 1400px)**:
   - Left Sidebar: 236px full 6-level hierarchy.
   - Right Inspector: 288px full multi-tab inspector.
   - Central Workspace: Flex remaining space (typically >= 876px).

2. **Medium Desktop (1200px to 1399px)**:
   - Left Sidebar: 220px compact hierarchy.
   - Right Inspector: 260px compact inspector.
   - Central Workspace: Spacious terminal view (>= 720px).

3. **Compact Desktop (960px to 1199px)**:
   - Left Sidebar: 200px compact hierarchy.
   - Right Inspector: Collapsed into on-demand overlay drawer.
   - Central Workspace: Dedicated space (>= 760px).

4. **Narrow Desktop (720px to 959px)**:
   - Left Sidebar: Compact 56px icon rail.
   - Right Inspector: Overlay drawer.
   - Central Workspace: Primary focus (>= 664px).

5. **Very Narrow Desktop (< 720px)**:
   - Left Sidebar & Inspector: Accessible through floating overlay drawers.
   - Central Workspace: Full-width single workspace.
   - Minimum usable viewport maintained without horizontal clipping.
