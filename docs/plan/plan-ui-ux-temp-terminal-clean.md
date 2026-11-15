# TEMP-TERMINAL — UI CLEANUP, ICON DEDUPLICATION & TOOLBAR REFINEMENT

You are a Principal UI/UX Designer and Senior Rust `egui` Engineer.

The Obsidian Slate - Quiet Sapphire redesign has already been implemented.

However, after inspecting the actual application, several visual problems remain, particularly excessive icons, duplicated controls, redundant toolbars, and unnecessary visual elements.

Your task is to refine the existing interface into a cleaner, more minimal, professional developer workspace.

**This is a focused UI refinement task, NOT a complete redesign.**

Follow the existing design system and preserve all application functionality.

# 1. PRIMARY OBJECTIVE

Eliminate visual clutter and unnecessary UI elements.

The most important requirement is:

**ONE ACTION, ONE PRIMARY LOCATION.**

The same action must not be unnecessarily displayed in multiple visible locations.

Every icon, button, toolbar, and visual element must have a clear purpose.

Prioritize:
- Minimalism.
- Visual clarity.
- Consistent icon placement.
- Reduced visual noise.
- Better spacing.
- Clear action hierarchy.
- Clean navigation.
- Maximum usable terminal workspace.

The application should feel closer to Linear and Zed in terms of visual simplicity.

# 2. ICON DEDUPLICATION — HIGHEST PRIORITY

Inspect the entire application and identify every duplicated or redundant icon.

Pay particular attention to:

- Global top navigation.
- Workspace tab bar.
- Terminal pane headers.
- Sidebar header and footer.
- Inspector header.
- Inspector tabs.
- Editor toolbar.
- Status bar.
- Context menus.
- Dialog actions.

The current interface displays similar controls in multiple places, especially split-pane controls and panel visibility controls.

Audit these carefully.

## Mandatory rules

1. The same action should have only one primary visible location.
2. Remove redundant icons from secondary toolbars.
3. Do not repeat global actions inside individual panes unless they serve a genuinely different scope.
4. Do not display duplicate sidebar or inspector visibility toggles.
5. Avoid repeating settings, search, split, layout, and panel controls unnecessarily.
6. Do not add icons merely to fill empty toolbar space.
7. Do not replace removed icons with decorative text or additional buttons.
8. Preserve access to every existing function through its designated primary control or existing command mechanism.

If the same action legitimately exists at different scopes, make the distinction clear.

For example:
- Global split action: creates or manages workspace splits.
- Pane-local action: controls only the current pane.

Do not retain both if they perform the same operation.

# 3. ESTABLISH A CLEAR ACTION HIERARCHY

Organize all controls according to their responsibility.

## A. Global Top Navigation

Keep only essential application-level controls:

- Application logo.
- Project switcher.
- Current project context.
- Global command/search entry.
- Essential application-level actions.

Remove redundant split, panel, or workspace controls from this area when they already have a better designated location.

Keep the top navigation visually lightweight.

## B. Workspace Tab Bar

This area should contain:

- Open workspace/session tabs.
- Active tab indicator.
- Tab close action.
- New tab action.
- Essential tab navigation controls.

Do not repeat global panel visibility controls here.

Do not repeat terminal pane controls unless they are genuinely tab-specific.

## C. Terminal Pane Header

Keep only controls relevant to the current pane:

- Shell/process identity.
- Pane-specific status.
- Essential pane actions.
- Optional context menu for secondary actions.

Do not repeat workspace-level split controls if they are already available elsewhere.

Avoid displaying multiple layout icons that perform identical operations.

## D. Left Sidebar

Keep the sidebar focused entirely on navigation and project organization.

Its bottom toolbar should not duplicate global actions already available elsewhere.

Remove redundant settings, search, add, or navigation icons when an equivalent primary action already exists.

Preserve all functionality through an appropriate primary location.

## E. Right Inspector

Keep:
- Inspector tabs.
- Essential panel-specific actions.
- A single clear panel close/collapse control.

Do not add duplicate visibility controls in the inspector header and global navigation.

## F. Status Bar

Use this area primarily for application and terminal status.

Avoid unnecessary action icons.

# 4. ICON DESIGN RULES

Establish a consistent icon system.

Requirements:

- Use one consistent icon family.
- Prefer existing vector icons.
- Use consistent stroke weight.
- Standard icon size: 14–16px.
- Toolbar icons: approximately 14–15px.
- Icon-only buttons: approximately 28–30px hit area.
- Use consistent visual alignment.
- Avoid mixing unrelated icon styles.
- Avoid unnecessary icon backgrounds.

Every icon-only interactive control must have:
- A clear semantic purpose.
- A tooltip describing its action.
- An accessible interaction state.
- Appropriate hover, pressed, and disabled states.

Use text labels for actions that are not immediately recognizable through their icons.

Do not use multiple different icons to represent the same action.

# 5. REMOVE UNNECESSARY VISUAL ELEMENTS

Review the entire interface for visual noise.

Remove or simplify:

- Redundant separators.
- Excessive borders.
- Unnecessary card containers.
- Repeated section icons.
- Decorative badges.
- Redundant status indicators.
- Oversized toolbar groups.
- Unnecessary empty placeholders.
- Repeated project/session metadata.

Avoid showing information in multiple locations when it provides no additional value.

Keep the interface visually calm and balanced.

# 6. SIDEBAR REFINEMENT

The sidebar should be compact and easy to scan.

Improve:
- Project hierarchy.
- Session tree.
- Agent list.
- Process list.
- Git summary.

Use icons only when they improve recognition.

Avoid adding an icon to every section heading if the heading is already clear.

Use typography, indentation, spacing, and subtle status indicators to establish hierarchy.

Maintain consistent row heights and alignment.

# 7. TOOLBAR SPACING AND COMPOSITION

Review all toolbars.

Requirements:
- Group related actions logically.
- Use consistent spacing.
- Avoid unnecessary button backgrounds.
- Keep primary actions visually distinguishable.
- Remove empty toolbar groups.
- Avoid distributing the same actions across several toolbars.

The interface must not look like a collection of icon buttons.

It should feel like a carefully organized professional desktop application.

# 8. TERMINAL WORKSPACE PRIORITY

The terminal must remain the dominant visual element.

Do not let toolbars, headers, or side panels consume unnecessary space.

Prioritize:
- Maximum terminal canvas area.
- Clear pane boundaries.
- Minimal pane headers.
- Readable shell information.
- Unobstructed terminal input.

Preserve all terminal functionality and rendering behavior.

# 9. RESPONSIVE CONSISTENCY

Verify that icon deduplication and toolbar simplification also work at different window sizes.

Check:
- 1920 x 1080.
- 1440 x 900.
- 1280 x 720.
- 1024 x 768.
- 800 x 600.

Do not reintroduce duplicated controls when switching responsive modes.

Ensure controls do not overlap or disappear without an alternative way to access the functionality.

# 10. FUNCTIONAL PRESERVATION

This is a presentation-layer refinement.

Strictly preserve:

- Terminal input and rendering.
- ConPTY integration.
- Clipboard functionality.
- Session management.
- Project navigation.
- Split-pane functionality.
- Editor and diff viewer.
- Git workflows.
- Agent management.
- Process supervision.
- Command palette.
- Quick Open.
- Settings.
- Existing keyboard shortcuts.

Removing a duplicate visual control must never remove the underlying functionality.

Do not change business logic or application protocols.

# 11. IMPLEMENTATION WORKFLOW

1. Inspect the complete existing UI implementation.
2. Inventory all visible icons and their associated actions.
3. Identify duplicate actions and redundant controls.
4. Define the primary location for each action.
5. Remove or consolidate unnecessary controls.
6. Refine toolbar spacing and visual hierarchy.
7. Inspect the complete application again.
8. Build and run the application.
9. Capture screenshots for visual comparison.
10. Perform functional regression tests.

Do not stop after cleaning only the top navigation.

Audit every screen and major component.

# 12. ACCEPTANCE CRITERIA

The task is complete only when:

- No unnecessary duplicate icons remain.
- Each action has a clear primary location.
- Toolbars are visually lightweight.
- Sidebar navigation is clean.
- Terminal pane headers are minimal.
- Inspector controls are consistent.
- Icon sizes and styles are uniform.
- No functionality has been removed.
- All responsive layouts remain usable.
- The terminal has maximum practical workspace space.
- The overall interface looks noticeably cleaner than before.

# 13. DELIVERABLES

Update:

`docs/ui-redesign/REDESIGN_PROGRESS.md`

Create:

`docs/ui-redesign/ICON_DEDUPLICATION_AND_UI_CLEANUP_REPORT.md`

Include:

- Identified duplicated controls.
- Removed or consolidated icons.
- Final action placement decisions.
- Modified files.
- Before/after screenshots.
- Responsive verification.
- Build and test results.
- Remaining issues.

Run:

`cargo test --workspace`

And:

`cargo build --release --bin temp-terminal-gui`

Keep `omaterm/` strictly read-only.

Do not modify the root `RUST REWRITE - TODO.md`.

# FINAL DIRECTIVE

Make the interface simpler, not more complicated.

Do not add new icons to replace removed icons.

Do not add unnecessary controls.

Do not redesign components that are already visually correct.

**Remove redundancy, establish clear action hierarchy, improve spacing, and let the content breathe.**

The final result should look like a premium, minimal, modern native developer workspace rather than an interface crowded with toolbar buttons.