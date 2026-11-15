use std::fs;
use std::process::Command;
use temp_terminal_rig::error::RigError;
use temp_terminal_rig::worktree::WorktreeManager;

fn setup_temp_repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path();

    let status = Command::new("git")
        .arg("init")
        .current_dir(path)
        .status()
        .expect("git init failed");
    assert!(status.success(), "git init failed");

    let status = Command::new("git")
        .args(["config", "user.name", "Rig Tester"])
        .current_dir(path)
        .status()
        .expect("git config user.name failed");
    assert!(status.success());

    let status = Command::new("git")
        .args(["config", "user.email", "rig-tester@example.com"])
        .current_dir(path)
        .status()
        .expect("git config user.email failed");
    assert!(status.success());

    // Create initial commit
    let init_file = path.join("README.md");
    fs::write(&init_file, "# Rig Test Repo\n").expect("failed to write README.md");

    let status = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(path)
        .status()
        .expect("git add failed");
    assert!(status.success());

    let status = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(path)
        .status()
        .expect("git commit failed");
    assert!(status.success());

    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(path)
        .output()
        .expect("git branch --show-current failed");
    let base_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(!base_branch.is_empty(), "base branch must not be empty");

    (dir, base_branch)
}

#[test]
fn test_create_and_list_worktree() {
    let (repo, base_branch) = setup_temp_repo();
    let repo_root = repo.path();

    let wt_path = WorktreeManager::create_worktree(repo_root, "T-1", &base_branch)
        .expect("create_worktree should succeed");

    assert!(wt_path.exists(), "worktree directory must exist");
    assert!(
        wt_path.ends_with(std::path::Path::new(".rig").join("wt").join("T-1")),
        "worktree path must match .rig/wt/T-1"
    );

    let list = WorktreeManager::list_worktrees(repo_root)
        .expect("list_worktrees should succeed");
    assert_eq!(list.len(), 1, "should list exactly 1 rig worktree");
    assert_eq!(list[0].ticket_id, "T-1");
    assert_eq!(list[0].branch, "rig/T-1");
    assert!(list[0].is_clean, "new worktree should be clean");

    // Make worktree dirty by creating an untracked file
    let dirty_file = wt_path.join("uncommitted.txt");
    fs::write(&dirty_file, "dirty content").expect("failed to write file");

    let list_dirty = WorktreeManager::list_worktrees(repo_root)
        .expect("list_worktrees should succeed");
    assert_eq!(list_dirty.len(), 1);
    assert!(
        !list_dirty[0].is_clean,
        "worktree with untracked file should not be clean"
    );
}

#[test]
fn test_has_landed_and_land_worktree() {
    let (repo, base_branch) = setup_temp_repo();
    let repo_root = repo.path();

    let wt_path = WorktreeManager::create_worktree(repo_root, "T-1", &base_branch)
        .expect("create_worktree should succeed");

    // In the worktree, add and commit a feature file
    let feat_file = wt_path.join("feature.txt");
    fs::write(&feat_file, "new feature").expect("failed to write feature.txt");

    let status = Command::new("git")
        .args(["add", "feature.txt"])
        .current_dir(&wt_path)
        .status()
        .expect("git add failed");
    assert!(status.success());

    let status = Command::new("git")
        .args(["commit", "-m", "add feature.txt"])
        .current_dir(&wt_path)
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Before landing: has_landed must be false
    let landed_before = WorktreeManager::has_landed(repo_root, &base_branch, "rig/T-1");
    assert!(!landed_before, "worktree should not be landed yet");

    // Land worktree
    let commit_hash = WorktreeManager::land_worktree(
        repo_root,
        "T-1",
        &base_branch,
        Some("land(T-1): squash merge feature"),
    )
    .expect("land_worktree should succeed");

    assert!(!commit_hash.is_empty(), "commit hash must not be empty");

    // After landing: has_landed must be true
    let landed_after = WorktreeManager::has_landed(repo_root, &base_branch, "rig/T-1");
    assert!(landed_after, "worktree should be landed after squash merge");

    // Verify file exists in main repo
    let merged_file = repo_root.join("feature.txt");
    assert!(merged_file.exists(), "merged file must exist in repo root");
    let content = fs::read_to_string(&merged_file).expect("failed to read merged file");
    assert_eq!(content, "new feature");
}

#[test]
fn test_clean_worktree_unmerged_safety() {
    let (repo, base_branch) = setup_temp_repo();
    let repo_root = repo.path();

    let wt_path = WorktreeManager::create_worktree(repo_root, "T-2", &base_branch)
        .expect("create_worktree should succeed");

    // Add unmerged commit
    let unmerged_file = wt_path.join("unmerged.txt");
    fs::write(&unmerged_file, "unmerged content").expect("failed to write unmerged.txt");

    let status = Command::new("git")
        .args(["add", "unmerged.txt"])
        .current_dir(&wt_path)
        .status()
        .expect("git add failed");
    assert!(status.success());

    let status = Command::new("git")
        .args(["commit", "-m", "unmerged commit"])
        .current_dir(&wt_path)
        .status()
        .expect("git commit failed");
    assert!(status.success());

    // Attempt clean without force: must fail with UnmergedWorktree
    let clean_res = WorktreeManager::clean_worktree(repo_root, "T-2", &base_branch, false);
    match clean_res {
        Err(RigError::UnmergedWorktree(_)) => {}
        other => panic!("expected Err(RigError::UnmergedWorktree), got {:?}", other),
    }

    assert!(wt_path.exists(), "worktree should still exist after rejected clean");
}

#[test]
fn test_clean_worktree_force_and_after_land() {
    let (repo, base_branch) = setup_temp_repo();
    let repo_root = repo.path();

    // 1. Force clean unmerged worktree
    let wt_path_3 = WorktreeManager::create_worktree(repo_root, "T-3", &base_branch)
        .expect("create_worktree should succeed");
    let file_3 = wt_path_3.join("unmerged_3.txt");
    fs::write(&file_3, "unmerged 3").unwrap();

    let status = Command::new("git")
        .args(["add", "unmerged_3.txt"])
        .current_dir(&wt_path_3)
        .status()
        .unwrap();
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "unmerged 3"])
        .current_dir(&wt_path_3)
        .status()
        .unwrap();
    assert!(status.success());

    WorktreeManager::clean_worktree(repo_root, "T-3", &base_branch, true)
        .expect("force clean should succeed");
    assert!(!wt_path_3.exists(), "worktree folder must be deleted after clean");

    // 2. Normal clean after landing
    let wt_path_4 = WorktreeManager::create_worktree(repo_root, "T-4", &base_branch)
        .expect("create_worktree should succeed");
    let file_4 = wt_path_4.join("landed_4.txt");
    fs::write(&file_4, "landed 4").unwrap();

    let status = Command::new("git")
        .args(["add", "landed_4.txt"])
        .current_dir(&wt_path_4)
        .status()
        .unwrap();
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "landed 4"])
        .current_dir(&wt_path_4)
        .status()
        .unwrap();
    assert!(status.success());

    WorktreeManager::land_worktree(repo_root, "T-4", &base_branch, None)
        .expect("land_worktree should succeed");

    WorktreeManager::clean_worktree(repo_root, "T-4", &base_branch, false)
        .expect("clean_worktree after landing should succeed without force");
    assert!(!wt_path_4.exists(), "worktree folder must be deleted");

    // List should now be empty
    let list = WorktreeManager::list_worktrees(repo_root)
        .expect("list_worktrees should succeed");
    assert!(list.is_empty(), "all worktrees should be cleaned up");
}

#[test]
fn test_land_worktree_dirty_repo_rejected() {
    let (repo, base_branch) = setup_temp_repo();
    let repo_root = repo.path();

    let wt_path = WorktreeManager::create_worktree(repo_root, "T-dirty", &base_branch)
        .expect("create_worktree should succeed");

    let feat = wt_path.join("feat.txt");
    fs::write(&feat, "feat content").unwrap();
    let _ = Command::new("git").args(["add", "feat.txt"]).current_dir(&wt_path).status().unwrap();
    let _ = Command::new("git").args(["commit", "-m", "feat"]).current_dir(&wt_path).status().unwrap();

    // Create uncommitted file in repo_root
    let dirty_file = repo_root.join("dirty_root.txt");
    fs::write(&dirty_file, "uncommitted changes").unwrap();

    let err = WorktreeManager::land_worktree(repo_root, "T-dirty", &base_branch, None);
    match err {
        Err(RigError::GitError(msg)) => {
            assert!(
                msg.contains("working directory has uncommitted changes"),
                "expected uncommitted changes error, got: {}",
                msg
            );
        }
        other => panic!("expected GitError with uncommitted changes, got {:?}", other),
    }

    // Remove dirty file and retry landing
    fs::remove_file(&dirty_file).unwrap();
    let hash = WorktreeManager::land_worktree(repo_root, "T-dirty", &base_branch, None)
        .expect("land should succeed once repo is clean");
    assert!(!hash.is_empty());
}

#[test]
fn test_land_worktree_conflict_rollback() {
    let (repo, base_branch) = setup_temp_repo();
    let repo_root = repo.path();

    let wt_path = WorktreeManager::create_worktree(repo_root, "T-conflict", &base_branch)
        .expect("create_worktree should succeed");

    // In worktree: modify README.md and commit
    let readme_wt = wt_path.join("README.md");
    fs::write(&readme_wt, "# Conflict from worktree\n").unwrap();
    let _ = Command::new("git").args(["add", "README.md"]).current_dir(&wt_path).status().unwrap();
    let _ = Command::new("git").args(["commit", "-m", "conflict wt"]).current_dir(&wt_path).status().unwrap();

    // In base repo: modify README.md and commit
    let readme_base = repo_root.join("README.md");
    fs::write(&readme_base, "# Conflict from base\n").unwrap();
    let _ = Command::new("git").args(["add", "README.md"]).current_dir(repo_root).status().unwrap();
    let _ = Command::new("git").args(["commit", "-m", "conflict base"]).current_dir(repo_root).status().unwrap();

    // Land should fail due to conflict and roll back via reset --hard HEAD
    let err = WorktreeManager::land_worktree(repo_root, "T-conflict", &base_branch, None);
    assert!(err.is_err(), "land should fail on conflict");

    // Verify rollback restored clean state with base content
    let readme_after = fs::read_to_string(&readme_base).unwrap();
    assert_eq!(readme_after.trim(), "# Conflict from base");

    let status_out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .unwrap();
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    let dirty_lines: Vec<&str> = status_str
        .lines()
        .filter(|l| !l.trim().ends_with(".rig/"))
        .collect();
    assert!(dirty_lines.is_empty(), "rollback should leave repo clean of conflict markers");
}
