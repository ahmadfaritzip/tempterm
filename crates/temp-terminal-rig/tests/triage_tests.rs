use temp_terminal_rig::{RigDatabase, TriageAction, TriageEngine};
use tempfile::tempdir;
use std::process::Command;
use std::fs;

fn init_git_repo_with_branch(repo_path: &std::path::Path, branch_name: &str) {
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .expect("git config name");
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .expect("git config email");

    fs::write(repo_path.join("README.md"), "initial").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(repo_path)
        .output()
        .expect("git commit");

    if branch_name != "main" {
        Command::new("git")
            .args(["checkout", "-b", branch_name])
            .current_dir(repo_path)
            .output()
            .expect("git checkout -b");
    }
}

#[test]
fn test_inspect_diff_stat_and_triage_evaluation() {
    let tmp = tempdir().unwrap();
    let repo_path = tmp.path();
    init_git_repo_with_branch(repo_path, "main");

    // Create a feature branch with some changes
    Command::new("git")
        .args(["checkout", "-b", "feature/abc"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    fs::write(repo_path.join("file1.txt"), "line1\nline2\nline3\n").unwrap();
    Command::new("git")
        .args(["add", "file1.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add file1"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let db_path = repo_path.join("rig.db");
    let mut db = RigDatabase::open_repo(&db_path).unwrap();

    let ticket = db.create_ticket(
        "Small change",
        "desc",
        "test",
        false,
        None,
        Some("main"),
        None,
    ).unwrap();

    let mut ticket_with_branch = ticket.clone();
    ticket_with_branch.branch = Some("feature/abc".to_string());
    db.update_ticket(&ticket_with_branch).unwrap();
    let ticket = ticket_with_branch;

    // Inspect diff stat
    let stat = TriageEngine::inspect_diff_stat(repo_path, &ticket).unwrap();
    assert_eq!(stat.files_changed, 1);
    assert!(stat.insertions > 0);

    // Evaluate triage with high threshold -> ApproveLocal
    let result_small = TriageEngine::evaluate_triage(repo_path, &mut db, "T-1", 100, None).unwrap();
    assert_eq!(result_small.action, TriageAction::ApproveLocal);

    // Evaluate triage with low threshold -> AssignReviewer
    let result_large = TriageEngine::evaluate_triage(repo_path, &mut db, "T-1", 1, None).unwrap();
    match result_large.action {
        TriageAction::AssignReviewer { harness, reason } => {
            assert_eq!(harness, "codex");
            assert!(!reason.is_empty());
        }
        _ => panic!("Expected AssignReviewer"),
    }
}

