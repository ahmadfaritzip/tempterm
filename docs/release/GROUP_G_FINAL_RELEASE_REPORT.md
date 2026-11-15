# GROUP G — FINALIZE, PACKAGE AND VERIFY WINDOWS RELEASE REPORT

**Date:** 2026-11-08  
**Status:** COMPLETE / PASS  
**Target Repository:** `D:/project/myproject/rewrite-terminal/`  
**Current Branch:** `feat/p1-02-core-proto`  
**Base HEAD:** `d4b9bdda64b7595df13c5b4a5e5899356675210c` (`fix(ui): adjust cursor blink rate to comfortable slower rhythm`)  
**Package Version:** `v0.1.0-preview`  
**Target Platform:** Windows x64 (`x86_64-pc-windows-msvc`)  
**Working Tree State:** All modifications strictly contained within `temp-terminal/crates/temp-terminal-ui/`, distribution scripts, and documentation artifacts.  
**Original Go Source:** `omaterm/` (100% READ-ONLY, unmodified, 0 byte diff).  
**Protected Files:** `RUST REWRITE - TODO.md` (untracked, unmodified).  

---

## 1. Executive Summary & Objective

Group G establishes the **final release engineering phase** for `temp-terminal`, delivering a production-quality, fully verified, portable Windows x64 release candidate (`temp-terminal-windows-x64.zip`).

All critical release-gate checks (G1) were closed with concrete empirical evidence on Windows 11. The portable ZIP package was built using locked dependencies, extracted to an isolated directory outside the source tree, and comprehensively smoke-tested across GUI, CLI, daemon, and persistent settings workflows.

---

## 2. Release-Gate Verification Closure (G1)

1. **G1.1 Agent Executable Resolution & Launch:**
   - Evaluated PATH executables via `where.exe`: detected installed system agents (`claude` in npm, `opencode` in bun, `pi` in agent bin, `agy` in Local AppData).
   - Confirmed `WorkspaceApp::trigger_new_agent` uses exact executable discovery and avoids reporting missing agents as successful launches.
2. **G1.2 Process Supervisor & Port Registration:**
   - Add Process registers process definitions in the workspace session manager with configurable autostart and restart policies (`Always`, `On Failure`, `Never`).
3. **G1.3 Status Bar Accuracy:**
   - Synchronized process and port counters dynamically in `refresh_snapshot()` against active sessions. Non-process terminals are excluded from process tallies.
4. **G1.4 Settings Restart Persistence:**
   - Verified live disk write-back to `%APPDATA%\temp-terminal\settings.json`.
   - Executed restart verification test: modified `theme = "dracula"`, `terminal.font_size = 15.5`, `terminal.cursor_blink = false`. Verified reloaded settings after process reload match saved values identically.
5. **G1.5 Evidence Completion:**
   - All 9 genuine desktop screenshots captured and verified under `temp-terminal/docs/ui-baseline/group_f/`.

---

## 3. Release Metadata & Manifests (G2)

- **Version:** `0.1.0` (Release tag: `v0.1.0-preview`).
- **Cargo Workspace:** `temp-terminal/Cargo.toml` (`version = "0.1.0"`, `edition = "2021"`).
- **GUI Binary Metadata:** Window title `temp-terminal` matching `0.1.0`.
- **CLI Output:** `temp-terminal --version` returns `temp-terminal 0.1.0`.
- **Daemon Output:** Default binds to `127.0.0.1:9099`.

---

## 4. Release Build & Binary Inventory (G3 – G4)

Built using `cargo build --release --workspace --locked`:

| Binary / Asset | Size (Bytes) | Relative Path | Purpose |
|---|---|---|---|
| `temp-terminal-gui.exe` | 6,788,608 | `temp-terminal-windows-x64/` | Native graphical desktop workspace |
| `temp-terminal-daemon.exe` | 2,003,456 | `temp-terminal-windows-x64/` | Headless session & process supervisor daemon |
| `temp-terminal.exe` | 1,261,056 | `temp-terminal-windows-x64/` | Command-line client for processes & mailboxes |
| `README.txt` | 1,191 | `temp-terminal-windows-x64/` | Quick-start, environment, and usage guide |
| **`temp-terminal-windows-x64.zip`** | **4,072,775** | `temp-terminal/dist/` | Complete standalone portable release package |

---

## 5. Extracted Package Smoke-Test Verification (G5)

The generated ZIP `temp-terminal-windows-x64.zip` was extracted to an isolated directory under `%TEMP%` (`C:\Users\Asus\AppData\Local\Temp\temp-terminal-release-smoke-test-*`) and tested independently:

1. **Package Extraction:** Archive decompressed cleanly with all 4 files intact.
2. **CLI Invocations:**
   - `temp-terminal.exe --help`: Successfully displayed CLI usage and commands.
   - `temp-terminal.exe --version`: Emitted `temp-terminal 0.1.0`.
3. **Daemon Background Startup & Ping:**
   - Started extracted `temp-terminal-daemon.exe` on loopback `127.0.0.1:9099`.
   - Executed `temp-terminal.exe ping` -> Received `daemon: ok`.
   - Executed `temp-terminal.exe shutdown` -> Daemon acknowledged and exited cleanly.
4. **GUI Application Launch:**
   - Started extracted `temp-terminal-gui.exe` as independent Windows process.
   - Process initialized graphics and message pump successfully without missing-DLL errors.
   - Clean shutdown executed.

---

## 6. Security Posture & Authenticode Status (G6)

- **Loopback Default:** Daemon IPC socket binds strictly to `127.0.0.1:9099`.
- **Unsigned Binaries Disclosure:** Binaries are unsigned (no private commercial Authenticode certificate). Windows SmartScreen may show an "Unrecognized app" prompt on initial run. Documented in `RELEASE_NOTES_v0.1.0-preview.md` and `README.txt`.
- **Resource Safety:** Child processes attached to Windows Job Objects for automatic termination. Memory caps enforced on terminal scrollback buffers.
- **Privacy Guarantee:** Package contains 0 credentials, 0 environment secrets, 0 `.env` files, and 0 user data.

---

## 7. SHA-256 Checksum Manifest (G7)

Stored in `temp-terminal/dist/SHA256SUMS.txt`:

```
CA33031AAFBA5594E43A84EF753AFA78D301FE372C95EBE046898C86B85E7A20  temp-terminal-windows-x64.zip
F5BFEC617964A2894C371F2B0A02022CCF8FE6D7E5BBB24EC7124F5FF009228E  temp-terminal-gui.exe
827DBB3CD91F012D5F7A58D401D423DEFEC5E60B6ED261EACC1FC237B16FF1F0  temp-terminal-daemon.exe
FF3D667DAA500ABF3E9A665A2F64E5989E251DFCE6CB1118126962BD3D87F632  temp-terminal.exe
149666F947CD8EE82DD7E2FF61DE6667A6C97FD4FD60878F7F8B1E5977CD3B3F  README.txt
```

---

## 8. Final Regression Suite (G9)

- `cargo fmt --check`: **PASS** (0 diffs).
- `cargo check --workspace --locked`: **PASS** (0 errors, 0 warnings).
- `cargo test --workspace --locked`: **PASS** (All 187 tests passing across all 9 crates).
- `omaterm/` and `RUST REWRITE - TODO.md`: **PASS** (100% untouched).

---

## 9. Final Verdict

**FINAL OVERALL VERDICT: PASS**

The portable release package `temp-terminal-windows-x64.zip` is verified, fully functional, and ready for end-user deployment.
