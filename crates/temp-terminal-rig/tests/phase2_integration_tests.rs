use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::layout::{LayoutAllocator, LayoutConfig, SlotRole};
use temp_terminal_rig::watcher::{StreamWatcher, SettlementKind};
use temp_terminal_rig::worktree::WorktreeManager;

#[test]
fn test_phase2_end_to_end_unified_lifecycle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_path = temp_dir.path();

    let status = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .status()
        .expect("git init success");
    assert!(status.success());

    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Rig Test"])
        .current_dir(repo_path)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "rig@test.com"])
        .current_dir(repo_path)
        .status();

    let readme_path = repo_path.join("README.md");
    std::fs::write(&readme_path, "Phase 2 Rig Test\n").unwrap();
    let _ = std::process::Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_path)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(repo_path)
        .status();

    // 1. Create ticket in RigDatabase
    let db = RigDatabase::open_repo(repo_path).unwrap();
    let ticket = db
        .create_ticket(
            "Implement phase 2 features",
            "Integration test description",
            "manager:p1",
            false,
            None,
            Some("main"),
            None,
        )
        .unwrap();
    assert_eq!(ticket.id, "T-1");
    assert_eq!(ticket.title, "Implement phase 2 features");

    // 2. Create isolated worktree for the ticket
    let wt_path = WorktreeManager::create_worktree(repo_path, &ticket.id, "main").unwrap();
    assert!(wt_path.exists());

    let worktrees = WorktreeManager::list_worktrees(repo_path).unwrap();
    assert_eq!(worktrees.len(), 1);
    let worktree_info = &worktrees[0];
    assert_eq!(worktree_info.branch, "rig/T-1");

    // 3. Commit changes in the worktree
    let feature_file = worktree_info.path.join("feature.rs");
    std::fs::write(&feature_file, "pub fn verified() -> bool { true }\n").unwrap();
    let _ = std::process::Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(&worktree_info.path)
        .status();
    let status_commit = std::process::Command::new("git")
        .args(["commit", "-m", "add verified feature"])
        .current_dir(&worktree_info.path)
        .status()
        .unwrap();
    assert!(status_commit.success());

    // 4. Simulate terminal VT output and verify settlement
    let vt_output = "\x1b[32mCompiling temp-terminal v0.1.0\x1b[0m\nFinished dev [unoptimized] target(s) in 0.42s\n❯ ";
    let settlement = StreamWatcher::detect_settlement(vt_output, "claude");
    assert_eq!(settlement, SettlementKind::Idle);

    let tail = StreamWatcher::extract_screen_tail(vt_output, 12);
    assert!(!tail.is_empty());

    // 5. Allocate multiplexer layout slots for Manager, Board, and Worker runs, validating bounds
    let config = LayoutConfig { pane_max: 2, tab_max: 8 };
    let location = LayoutAllocator::allocate_slot(&SlotRole::Worker(2), &config);
    let bounds = LayoutAllocator::compute_rect_bounds(&location);
    assert!(bounds.w > 0.0);
    assert!(bounds.h > 0.0);

    // 6. Land worktree changes back to base branch and verify has_landed()
    assert!(!WorktreeManager::has_landed(repo_path, "main", &worktree_info.branch));
    let _hash = WorktreeManager::land_worktree(repo_path, &ticket.id, "main", Some("land(T-1): feature complete")).unwrap();
    assert!(WorktreeManager::has_landed(repo_path, "main", &worktree_info.branch));

    // 7. Clean worktree cleanly
    WorktreeManager::clean_worktree(repo_path, &ticket.id, "main", false).unwrap();
    assert!(!worktree_info.path.exists());

    // 8. Verify ticket status or database integrity
    let fetched_ticket = db.get_ticket(&ticket.id).unwrap();
    assert_eq!(fetched_ticket.title, "Implement phase 2 features");
}
