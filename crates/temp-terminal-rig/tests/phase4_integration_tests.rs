//! Phase 4 End-to-End Integration Tests.
//!
//! Validates the complete lifecycle:
//! 1. Temporary Git repository initialization on branch main.
//! 2. RigDatabase open and connection initialization.
//! 3. Ticket creation across all lifecycle states:
//!    - T-1: NeedsYou
//!    - T-2: Working on feature branch
//!    - T-3: Review (Ready)
//!    - T-4: Queued (after: T-2)
//!    - T-5: To do
//!    - T-6: Done
//! 4. TUI Board Renderer output, ANSI color groups, spinner frames, done filter, and activity stream.
//! 5. Git diff inspection and TriageEngine evaluation (AssignReviewer for large diff, ApproveLocal for small diff).
//! 6. UI state consistency via RigBoardState alert checking and toast notifications.
//! 7. Anti-slop compliance verification across Phase 4 source and test code.

use std::fs;
use std::path::Path;
use std::process::Command;
use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::ticket::TicketStatus;
use temp_terminal_rig::triage::{TriageAction, TriageEngine};
use temp_terminal_rig::tui::{BoardRenderOptions, BoardTuiRenderer};
use temp_terminal_ui::rig_board::RigBoardState;

#[test]
fn test_phase4_end_to_end_lifecycle() {
    let temp_dir = tempfile::tempdir().expect("create temporary directory");
    let repo_root = temp_dir.path();

    // 1. Initialize temporary Git repository with initial commit on main
    let init_status = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_root)
        .status()
        .expect("git init -b main");
    assert!(init_status.success(), "git init must succeed");

    let _ = Command::new("git")
        .args(["config", "user.name", "Rig Tester"])
        .current_dir(repo_root)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "rig@example.com"])
        .current_dir(repo_root)
        .status();

    let readme_path = repo_root.join("README.md");
    fs::write(&readme_path, "Phase 4 Rig Integration Workspace\n").expect("write readme");

    let add_status = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_root)
        .status()
        .expect("git add readme");
    assert!(add_status.success());

    let commit_status = Command::new("git")
        .args(["commit", "-m", "chore: initial repository commit"])
        .current_dir(repo_root)
        .status()
        .expect("git commit initial");
    assert!(commit_status.success());

    // 2. Open RigDatabase on the repository
    let mut db = RigDatabase::open_repo(repo_root).expect("open repo database");

    // 3. Create tickets with complete status cycle
    // T-1: Needs attention (NeedsYou)
    let t1 = db
        .create_ticket(
            "Clarify authentication API contract",
            "User input required on OAuth callback URL handling",
            "claude",
            false,
            None,
            Some("main"),
            Some("auth-flow"),
        )
        .expect("create ticket T-1");
    assert_eq!(t1.id, "T-1");
    db.update_ticket_status(&t1.id, TicketStatus::NeedsYou)
        .expect("set T-1 to NeedsYou");

    // T-2: Under active development on feature branch
    let t2 = db
        .create_ticket(
            "Implement multi-buffer terminal multiplexer",
            "Porting terminal buffer slicing and scrollback",
            "codex",
            false,
            None,
            Some("main"),
            Some("term-core"),
        )
        .expect("create ticket T-2");
    assert_eq!(t2.id, "T-2");
    let mut t2_record = db.get_ticket(&t2.id).expect("fetch T-2");
    t2_record.branch = Some("feature/task-2".to_string());
    t2_record.status = TicketStatus::Working;
    db.update_ticket(&t2_record).expect("update T-2");

    // T-3: Ready for review (Review)
    let t3 = db
        .create_ticket(
            "Review session persistence sqlite schema",
            "Database migrations ready for architectural check",
            "pi",
            true,
            None,
            Some("main"),
            Some("db-schema"),
        )
        .expect("create ticket T-3");
    assert_eq!(t3.id, "T-3");
    db.update_ticket_status(&t3.id, TicketStatus::Review)
        .expect("set T-3 to Review");

    // T-4: Queued behind T-2 (after: Some("T-2"))
    let t4 = db
        .create_ticket(
            "Write integration benchmarks for multiplexer",
            "Requires multi-buffer terminal implementation to land first",
            "worker-1",
            false,
            Some("T-2"),
            Some("main"),
            Some("term-core"),
        )
        .expect("create ticket T-4");
    assert_eq!(t4.id, "T-4");

    // T-5: Ready to do (To do)
    let t5 = db
        .create_ticket(
            "Configure telemetry logger format",
            "Set structured JSON log parameters for daemon",
            "worker-2",
            false,
            None,
            Some("main"),
            None,
        )
        .expect("create ticket T-5");
    assert_eq!(t5.id, "T-5");

    // T-6: Completed (Done)
    let t6 = db
        .create_ticket(
            "Set up repository license and metadata",
            "Add MIT license and cargo workspace definition",
            "claude",
            false,
            None,
            Some("main"),
            None,
        )
        .expect("create ticket T-6");
    assert_eq!(t6.id, "T-6");
    db.update_ticket_status(&t6.id, TicketStatus::Done)
        .expect("set T-6 to Done");

    // Verify database grouping semantics for all tickets
    let fresh_t1 = db.get_ticket("T-1").unwrap();
    let fresh_t2 = db.get_ticket("T-2").unwrap();
    let fresh_t3 = db.get_ticket("T-3").unwrap();
    let fresh_t4 = db.get_ticket("T-4").unwrap();
    let fresh_t5 = db.get_ticket("T-5").unwrap();
    let fresh_t6 = db.get_ticket("T-6").unwrap();

    assert_eq!(db.group_of(&fresh_t1).unwrap(), "Needs you");
    assert_eq!(db.group_of(&fresh_t2).unwrap(), "Working");
    assert_eq!(db.group_of(&fresh_t3).unwrap(), "Ready");
    assert_eq!(db.group_of(&fresh_t4).unwrap(), "Queued");
    assert_eq!(db.group_of(&fresh_t5).unwrap(), "To do");
    assert_eq!(db.group_of(&fresh_t6).unwrap(), "Done");

    // Send inter-agent message to verify activity stream rendering
    db.send_message(
        Some("T-2"),
        "codex",
        "manager",
        "Diff exceeds threshold, request assigned reviewer",
        Some("term-core"),
    )
    .expect("send test message");

    // 4. Verify TUI Board Renderer
    // Test without Done tickets
    let options_hide_done = BoardRenderOptions {
        show_done: false,
        show_activity: true,
        max_activity_items: 5,
        spinner_tick: 2,
    };
    let rendered_board =
        BoardTuiRenderer::render_board(&mut db, &options_hide_done).expect("render board hide done");

    // Verify ANSI color tags and group titles
    assert!(
        rendered_board.contains("\x1b[31m● [Needs you] (1)\x1b[0m"),
        "Needs you group must use red ANSI escape code"
    );
    assert!(
        rendered_board.contains("\x1b[33m"),
        "Working group must use yellow ANSI escape code"
    );
    assert!(
        rendered_board.contains("[Working] (1)"),
        "Working group title and count must match"
    );
    assert!(
        rendered_board.contains("\x1b[32m◆ [Ready] (1)\x1b[0m"),
        "Ready group must use green ANSI escape code"
    );
    assert!(
        rendered_board.contains("\x1b[90m◷ [Queued] (1)\x1b[0m"),
        "Queued group must use muted ANSI escape code"
    );
    assert!(
        rendered_board.contains("\x1b[37m○ [To do] (1)\x1b[0m"),
        "To do group must use white ANSI escape code"
    );

    // Verify spinner frame animation calculation
    let expected_spinner = BoardTuiRenderer::SPINNER_FRAMES[options_hide_done.spinner_tick % 10];
    assert!(
        rendered_board.contains(expected_spinner),
        "Working section must display current spinner tick frame"
    );

    // Verify T-6 does not show when show_done is false
    assert!(
        !rendered_board.contains("[T-6]"),
        "T-6 Done ticket must not be visible when show_done is false"
    );
    assert!(
        !rendered_board.contains("[Done]"),
        "Done section header must not be rendered when show_done is false"
    );

    // Verify activity stream contains recent inter-agent message
    assert!(
        rendered_board.contains("--- Activity Stream ---"),
        "Activity stream header must be present"
    );
    assert!(
        rendered_board.contains("[T-2] codex -> manager: Diff exceeds threshold"),
        "Activity stream must display logged inter-agent message"
    );

    // Test with Done tickets enabled
    let options_show_done = BoardRenderOptions {
        show_done: true,
        show_activity: false,
        max_activity_items: 5,
        spinner_tick: 5,
    };
    let rendered_with_done =
        BoardTuiRenderer::render_board(&mut db, &options_show_done).expect("render board show done");

    assert!(
        rendered_with_done.contains("\x1b[90m✓ [Done] (1)\x1b[0m"),
        "Done group must appear when show_done is true"
    );
    assert!(
        rendered_with_done.contains("[T-6] Set up repository license and metadata"),
        "T-6 ticket must be rendered in Done group"
    );

    // 5. Simulate Git Changes and Evaluate Triage
    // Create branch feature/task-2 and commit modifications exceeding 50 lines
    let branch_status = Command::new("git")
        .args(["checkout", "-b", "feature/task-2"])
        .current_dir(repo_root)
        .status()
        .expect("git checkout -b feature/task-2");
    assert!(branch_status.success());

    let mod_file_a = repo_root.join("multiplexer_part_a.rs");
    let mod_file_b = repo_root.join("multiplexer_part_b.rs");

    let mut body_a = String::new();
    for i in 1..=35 {
        body_a.push_str(&format!("pub fn buffer_chunk_{}() -> usize {{ {} }}\n", i, i));
    }
    fs::write(&mod_file_a, body_a).expect("write file a");

    let mut body_b = String::new();
    for i in 1..=35 {
        body_b.push_str(&format!("pub fn slice_chunk_{}() -> usize {{ {} }}\n", i, i));
    }
    fs::write(&mod_file_b, body_b).expect("write file b");

    let add_changes = Command::new("git")
        .args(["add", "multiplexer_part_a.rs", "multiplexer_part_b.rs"])
        .current_dir(repo_root)
        .status()
        .expect("git add new files");
    assert!(add_changes.success());

    let commit_changes = Command::new("git")
        .args(["commit", "-m", "feat(mux): implement buffer slicing parts"])
        .current_dir(repo_root)
        .status()
        .expect("git commit mux");
    assert!(commit_changes.success());

    // Switch back to main to inspect diff between main and feature branch
    let checkout_main = Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .status()
        .expect("git checkout main");
    assert!(checkout_main.success());

    // Evaluate triage for T-2 with threshold 50 lines
    let triage_res = TriageEngine::evaluate_triage(repo_root, &mut db, "T-2", 50, None)
        .expect("evaluate triage for T-2");

    assert_eq!(triage_res.diff_stat.files_changed, 2);
    assert_eq!(triage_res.diff_stat.insertions, 70);
    assert_eq!(triage_res.diff_stat.deletions, 0);

    match triage_res.action {
        TriageAction::AssignReviewer { harness, reason } => {
            assert!(
                !harness.is_empty(),
                "Assigned reviewer harness must not be empty"
            );
            assert!(
                reason.contains("exceed threshold"),
                "Triage reason must explain threshold exceeded condition"
            );
        }
        TriageAction::ApproveLocal => {
            panic!("Expected AssignReviewer for 70 lines diff with 50 threshold");
        }
    }

    // Now test small change under threshold on branch feature/small-tweak
    let small_branch_status = Command::new("git")
        .args(["checkout", "-b", "feature/small-tweak"])
        .current_dir(repo_root)
        .status()
        .expect("git checkout -b feature/small-tweak");
    assert!(small_branch_status.success());

    let small_file = repo_root.join("patch.rs");
    fs::write(&small_file, "pub const PATCH_VERSION: u32 = 1;\n").expect("write small patch");

    let _ = Command::new("git")
        .args(["add", "patch.rs"])
        .current_dir(repo_root)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "fix: small one line tweak"])
        .current_dir(repo_root)
        .status();

    let _ = Command::new("git")
        .args(["checkout", "main"])
        .current_dir(repo_root)
        .status();

    let t7 = db
        .create_ticket(
            "Apply patch version increment",
            "Single line constant update",
            "claude",
            false,
            None,
            Some("main"),
            None,
        )
        .expect("create T-7");
    let mut t7_record = db.get_ticket(&t7.id).expect("fetch T-7");
    t7_record.branch = Some("feature/small-tweak".to_string());
    db.update_ticket(&t7_record).expect("update T-7");

    let small_triage = TriageEngine::evaluate_triage(repo_root, &mut db, &t7.id, 50, None)
        .expect("evaluate triage for small change");

    assert_eq!(small_triage.diff_stat.files_changed, 1);
    assert_eq!(small_triage.diff_stat.insertions, 1);
    assert_eq!(
        small_triage.action,
        TriageAction::ApproveLocal,
        "Small changes under line threshold must be approved locally"
    );

    // 6. Verify UI State Consistency via RigBoardState
    let mut ui_state = RigBoardState::new(repo_root.to_path_buf());
    assert!(
        ui_state.pending_toasts.is_empty(),
        "New RigBoardState starts with no pending toasts"
    );

    // check_alerts should inspect database and identify T-1 as NeedsYou
    ui_state.check_alerts(&mut db);

    assert!(
        !ui_state.pending_toasts.is_empty(),
        "Pending toasts must contain alert for NeedsYou ticket"
    );
    assert!(
        ui_state.pending_toasts.iter().any(|m| m.contains("T-1")),
        "Toast message must reference ticket ID T-1"
    );
    assert!(
        ui_state.pending_toasts.iter().any(|m| m.contains("needs_you")),
        "Toast message must indicate needs_you status"
    );

    // Pop toast notification
    let popped = ui_state.pop_toast();
    assert!(popped.is_some(), "Toast alert was successfully popped");
    assert!(
        ui_state.pending_toasts.is_empty(),
        "Popping all toasts leaves queue clear"
    );
}

#[test]
fn test_phase4_anti_slop_compliance() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("resolve workspace root");

    let phase4_files = [
        "crates/temp-terminal-rig/src/tui.rs",
        "crates/temp-terminal-rig/src/triage.rs",
        "crates/temp-terminal-rig/src/bin/rig.rs",
        "crates/temp-terminal-rig/tests/phase4_integration_tests.rs",
        "crates/temp-terminal-rig/tests/tui_tests.rs",
        "crates/temp-terminal-rig/tests/triage_tests.rs",
        "crates/temp-terminal-rig/tests/e2e_cli_tests.rs",
        "crates/temp-terminal-ui/src/rig_board.rs",
        "crates/temp-terminal-ui/tests/rig_board_ui_tests.rs",
    ];

    for relative_path in phase4_files {
        let file_path = workspace_root.join(relative_path);
        let content = fs::read_to_string(&file_path)
            .unwrap_or_else(|e| panic!("failed to read file {}: {}", file_path.display(), e));

        // 1. Zero em dash characters
        assert!(
            !content.contains('\u{2014}'),
            "File {} contains forbidden em dash (\\u2014)",
            file_path.display()
        );

        // 2. Zero decorative emoji
        for (line_idx, line) in content.lines().enumerate() {
            for ch in line.chars() {
                let code = ch as u32;
                let is_emoji = (0x1F300..=0x1F9FF).contains(&code)
                    || (0x2600..=0x26FF).contains(&code)
                    || (0x2700..=0x27BF).contains(&code);

                // Allow checkmark glyph U+2713 and gauge glyphs U+2758, U+275A
                let is_allowed_symbol = code == 0x2713 || code == 0x2758 || code == 0x275A;

                if is_emoji && !is_allowed_symbol {
                    panic!(
                        "File {} line {} contains forbidden decorative emoji '{}' (U+{:04X})",
                        file_path.display(),
                        line_idx + 1,
                        ch,
                        code
                    );
                }
            }
        }

        // 3. Zero hardcoded raw RGB in GUI code
        if relative_path.contains("rig_board.rs") {
            assert!(
                !content.contains("Color32::from_rgb"),
                "rig_board.rs must not contain hardcoded Color32::from_rgb"
            );
            assert!(
                !content.contains("Color32::from_rgba"),
                "rig_board.rs must not contain hardcoded Color32::from_rgba"
            );
        }
    }
}
