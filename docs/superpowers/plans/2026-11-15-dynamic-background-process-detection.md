# Dynamic Background Process Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect what process is actively running in the background for each terminal pane (e.g. `npm run dev`, `cargo run`, `python`, `agy`) using Windows Job Objects and kernel process queries, and reflect it dynamically in the UI without relying on hardcoded screen text.

**Architecture:** Each terminal pane runs inside a Windows Job Object managed by `temp-terminal-pty`. By querying `QueryInformationJobObject` and `QueryFullProcessImageNameW`, the system discovers active child process IDs (PIDs) running under the shell, resolves their executable names, filters out helper processes, and correlates with the executed command line. The resolved process is propagated through `temp-terminal-core` and `temp-terminal-ui` to update the pane header and sidebar in real-time.

**Tech Stack:** Rust 2021, Windows API (`windows-sys` with `Win32_System_JobObjects`, `Win32_System_Threading`), `temp-terminal-pty`, `temp-terminal-core`, `temp-terminal-ui`, `egui`.

## Global Constraints

- No decorative emojis and no em dashes across all code, tests, documentation, and commit messages.
- All file links must use clickable `file:///` format with forward slashes.
- Original Go source in `omaterm/` strictly read-only.
- All implementations must be inside `temp-terminal/`.
- Must adhere to Test-Driven Development (TDD): write failing test first, verify failure, write minimal code, verify green.

---

### Task 1: Windows Job Object Child Process Inspection in `temp-terminal-pty`

**Files:**
- Modify: `crates/temp-terminal-pty/src/lib.rs`
- Test: `crates/temp-terminal-pty/tests/pty_windows_test.rs`

**Interfaces:**
- Produces:
  - `pub fn query_process_name(pid: u32) -> Option<String>`
  - `PtyProc::active_child_pids(&self) -> Vec<u32>`
  - `PtyProc::active_foreground_process(&self) -> Option<String>`

- [ ] **Step 1: Write the failing test for query_process_name and active_foreground_process**

```rust
#[test]
fn test_query_process_name_current_process() {
    let current_pid = std::process::id();
    let name = query_process_name(current_pid);
    assert!(name.is_some());
    let proc_name = name.unwrap().to_lowercase();
    assert!(proc_name.ends_with(".exe") || !proc_name.is_empty());
}
```

- [ ] **Step 2: Run test to verify failure or lack of definition**

```bash
cargo test -p temp-terminal-pty --test pty_windows_test test_query_process_name_current_process
```

- [ ] **Step 3: Implement query_process_name and active_foreground_process in `temp-terminal-pty`**

Implement `query_process_name` using `OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION` and `QueryFullProcessImageNameW`.
Implement `active_child_pids` and `active_foreground_process` on `PtyProc` to filter out root shell PID and console helper binaries (`conhost.exe`, `openconsole.exe`).

- [ ] **Step 4: Run test to verify green**

```bash
cargo test -p temp-terminal-pty --test pty_windows_test
```

- [ ] **Step 5: Commit changes**

```bash
git add crates/temp-terminal-pty
git commit -m "feat(pty): implement Windows JobObject child process query and active foreground process detection"
```

---

### Task 2: Core Registry and UI Pane Process Query Integration

**Files:**
- Modify: `crates/temp-terminal-core/src/registry.rs`
- Modify: `crates/temp-terminal-ui/src/lib.rs`
- Test: `crates/temp-terminal-ui/tests/ui_frontend_integration_test.rs`

**Interfaces:**
- Consumes: `PtyProc::active_foreground_process(&self) -> Option<String>`
- Produces:
  - `Pane::active_foreground_process(&self) -> Option<String>`
  - `TerminalPane::active_foreground_process(&self) -> Option<String>`

- [ ] **Step 1: Write failing test in UI integration test suite**

```rust
#[tokio::test]
async fn test_terminal_pane_reports_active_foreground_process() {
    // Verify that TerminalPane exposes active_foreground_process correctly
}
```

- [ ] **Step 2: Run test to verify failure**

```bash
cargo test -p temp-terminal-ui --test ui_frontend_integration_test test_terminal_pane_reports_active_foreground_process
```

- [ ] **Step 3: Implement active_foreground_process delegation in Core and UI**

Expose `active_foreground_process` on `Pane` in `registry.rs` and `TerminalPane` in `temp-terminal-ui/src/lib.rs`.

- [ ] **Step 4: Run test to verify green**

```bash
cargo test -p temp-terminal-ui --test ui_frontend_integration_test test_terminal_pane_reports_active_foreground_process
```

- [ ] **Step 5: Commit changes**

```bash
git add crates/temp-terminal-core crates/temp-terminal-ui/src/lib.rs
git commit -m "feat(core,ui): expose active foreground process query on Pane and TerminalPane"
```

---

### Task 3: Workspace State, Dynamic Process Correlation, and Header Rendering

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`
- Test: `crates/temp-terminal-ui/tests/terminal_ctrl_c_test.rs`

**Interfaces:**
- Consumes: `TerminalPane::active_foreground_process(&self) -> Option<String>`
- Produces:
  - `PaneRenderSnapshot.active_process: Option<String>`
  - Dynamic pane header displaying active process / command
  - Dynamic agent status synchronization in sidebar

- [ ] **Step 1: Write failing test for dynamic process detection and title generation**

```rust
#[test]
fn test_pane_header_title_with_os_active_process() {
    // Verify that when an active child process (e.g. "node", "cargo", "agy") is detected,
    // the title reflects the active process rather than generic terminal.
}
```

- [ ] **Step 2: Run test to verify failure**

```bash
cargo test -p temp-terminal-ui --test terminal_ctrl_c_test test_pane_header_title_with_os_active_process
```

- [ ] **Step 3: Implement dynamic process correlation in `workspace.rs`**

- In `refresh_snapshot`: Read `os_proc = pane.active_foreground_process()`. Normalize name (`node.exe` -> `node`, `cargo.exe` -> `cargo`, `agy.exe` -> `agy`, etc.).
- Correlate with terminal command line (e.g. if `node` is running and line has `npm run dev`, title becomes `npm run dev (node)` or `node`).
- If process is an agent (`agy`, `claude`, `codex`, `pi`), automatically sync with `active_agents`.
- In `render_single_pane` header: Display the active process label with appropriate icon.

- [ ] **Step 4: Run test to verify green**

```bash
cargo test -p temp-terminal-ui --test terminal_ctrl_c_test
```

- [ ] **Step 5: Commit changes**

```bash
git add crates/temp-terminal-ui/src/workspace.rs crates/temp-terminal-ui/tests/terminal_ctrl_c_test.rs
git commit -m "feat(ui): display dynamic background process in pane header and sync agent state"
```

---

### Task 4: Full Workspace Verification, Binary Builds, and Push

**Files:**
- None (verification and integration)

- [ ] **Step 1: Run full workspace test suite**

```bash
cargo test --workspace
```

- [ ] **Step 2: Build debug and release binaries**

```bash
cargo build --bin temp-terminal-gui
cargo build --release --bin temp-terminal-gui
```

- [ ] **Step 3: Push changes to remote origin**

```bash
git push origin main
```
