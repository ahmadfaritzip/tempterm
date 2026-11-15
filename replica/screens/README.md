# Reference Screen Mockups: herdr-rig

Dokumen ini mendokumentasikan tata letak visual (*visual layout*) dari antarmuka herdr-rig asli untuk keperluan acuan paritas clean-room.

---

### S01 / S02: TUI Live Board View (`rig board`)

```text
 ▲ rig  temp-terminal                                                       14:32
 ● 1 needs you   ◐ 2 working   ◆ 1 ready   ◷ 1 queued   ○ 3 to do   ✓ 8 done
 review · local · worktree · pane · claude → codex

 NEEDS YOU 1 ────────────────────────────────────────────────────────────────────
 ● T-4  Add OAuth callback handler                                            12m
   ⎇ rig/T-4  ! blocked: missing client secret in environment      worker  claude

 WORKING 2 ──────────────────────────────────────────────────────────────────────
 ⠋ T-2  Implement ConPTY resize handler                                        4m
   ⎇ rig/T-2  › running unit tests                                 worker  codex
 ⠋ T-5  Update settings schema docs                                            1m
   ⎇ rig/T-5  › drafting markdown sections                         worker  pi

 READY 1 ────────────────────────────────────────────────────────────────────────
 ◆ T-1  Extract SQLite state engine                                           28m
   ⎇ rig/T-1  done: verification passed                            worker  claude

 QUEUED 1 ───────────────────────────────────────────────────────────────────────
 ◷ T-3  Integrate board UI into workspace                                     35m
   waiting on T-1 (ready), T-2 (working)

 TO DO 3 ────────────────────────────────────────────────────────────────────────
 ○ T-6  Support cross-repo hub registry
 ○ T-7  Add desktop toast notifications
 ○ T-8  Benchmark worktree creation overhead

 ACTIVITY ───────────────────────────────────────────────────────────────────────
 14:31  T-2 worker    done: resize tests green; verify: cargo test -p ui; risks: none
 14:28  T-4 worker    blocked: missing OAuth client secret in .env
 14:20  T-1 reviewer  PASS: acceptance criteria fully satisfied, diff is clean
 14:15  T-1 manager   dispatching T-1 to reviewer (codex)
```

---

### S03: Desktop App GUI Mod View (GPUI / Native Panel)

```text
+-------------------------------------------------------------------------------+
| [▲ rig] temp-terminal                          [+ New Ticket] [Settings] [X] |
+-------------------------------------------------------------------------------+
| ▼ Needs you (1)                                                               |
|   +-------------------------------------------------------------------------+ |
|   | ● T-4  Add OAuth callback handler                                   12m | |
|   |   ⎇ rig/T-4 · [Claude] [High]                                           | |
|   |   Missing client secret in environment                                  | |
|   |                                          [Answer] [Open Pane] [Kill]    | |
|   +-------------------------------------------------------------------------+ |
|                                                                               |
| ▼ Working (2)                                                                 |
|   +-------------------------------------------------------------------------+ |
|   | ⠋ T-2  Implement ConPTY resize handler                               4m | |
|   |   ⎇ rig/T-2 · [Codex] [Med]                                             | |
|   |   › running unit tests                   [Steer] [Open Pane]            | |
|   +-------------------------------------------------------------------------+ |
|   +-------------------------------------------------------------------------+ |
|   | ⠋ T-5  Update settings schema docs                                   1m | |
|   |   ⎇ rig/T-5 · [Pi] [Low]                                                | |
|   |   › drafting markdown sections           [Steer] [Open Pane]            | |
|   +-------------------------------------------------------------------------+ |
|                                                                               |
| ▶ Ready (1)                                                                   |
| ▶ Queued (1)                                                                  |
| ▶ To do (3)                                                                   |
| ▶ Done (8)                                                                    |
+-------------------------------------------------------------------------------+
```

---

### S04 / S05: Terminal Multiplexing Layout (Manager 50% Left, Board + Runs Right)

```text
+-----------------------------------+-----------------------------------+
| MANAGER SESSION (50% WIDTH)       | BOARD PANE (50% WIDTH, TOP 50%)   |
|                                   | ▲ rig  temp-terminal              |
| [manager@temp-terminal]$ rig ls   | ● 1 needs you   ◐ 1 working       |
| T-1  ready    review  SQLite DB   | ───────────────────────────────── |
| T-2  working          ConPTY fix  | WORKING 1                         |
| T-3  queued           Board UI    | ⠋ T-2  Implement ConPTY resize    |
|                                   +-----------------------------------+
| [manager@temp-terminal]$ _        | WORKER PANE (BOTTOM 50%)          |
|                                   | [T-2:worker@wt/T-2]$ cargo test   |
|                                   | running 12 tests                  |
|                                   | test test_conpty_resize ... ok    |
|                                   |                                   |
+-----------------------------------+-----------------------------------+
```
