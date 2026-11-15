use std::process::Command;
use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::ticket::{TicketFilter, TicketStatus};
use tempfile::tempdir;

#[test]
fn test_end_to_end_rig_lifecycle_flow() {
    let dir = tempdir().unwrap();
    let db = RigDatabase::open_repo(dir.path()).unwrap();

    // 1. Simpan konfigurasi awal repo
    db.set_setting("worker.harness", "claude").unwrap();
    db.set_setting("reviewer.harness", "codex").unwrap();
    assert_eq!(
        db.get_setting_or_default("worker.harness").unwrap(),
        "claude"
    );

    // 2. Buat tiket pertama T-1
    let t1 = db
        .create_ticket(
            "Build Database Engine",
            "Goal: SQLite engine",
            "manager:p1",
            true,
            None,
            Some("main"),
            None,
        )
        .unwrap();
    assert_eq!(t1.id, "T-1");
    assert_eq!(db.group_of(&t1).unwrap(), "To do");

    // 3. Buat tiket kedua T-2 yang bergantung pada T-1
    let t2 = db
        .create_ticket(
            "Build TUI Board",
            "Goal: Render board",
            "manager:p1",
            false,
            Some("T-1"),
            Some("main"),
            None,
        )
        .unwrap();
    assert_eq!(t2.id, "T-2");
    assert_eq!(db.group_of(&t2).unwrap(), "Queued");

    // 4. Update status T-1 menjadi working
    db.update_ticket_status(&t1.id, TicketStatus::Working)
        .unwrap();
    let t1_working = db.get_ticket(&t1.id).unwrap();
    assert_eq!(db.group_of(&t1_working).unwrap(), "Working");

    // 5. Worker mengirim pesan progres
    db.send_message(
        Some(&t1.id),
        "T-1/worker",
        "manager:p1",
        "milestone: schema completed",
        None,
    )
    .unwrap();
    let msgs = db.read_inbox("manager:p1", true, None).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].body, "milestone: schema completed");

    // 6. Worker selesai dan menyerahkan untuk review
    db.update_ticket_status(&t1.id, TicketStatus::Review)
        .unwrap();
    let t1_review = db.get_ticket(&t1.id).unwrap();
    assert_eq!(db.group_of(&t1_review).unwrap(), "Ready");

    // 7. Reviewer PASS dan Manager menandai Done
    db.update_ticket_status(&t1.id, TicketStatus::Done).unwrap();
    let t1_done = db.get_ticket(&t1.id).unwrap();
    assert_eq!(db.group_of(&t1_done).unwrap(), "Done");

    // 8. T-2 otomatis terbebas dari antrean
    assert_eq!(db.group_of(&t2).unwrap(), "To do");

    // 9. List tickets filter
    let active_tickets = db
        .list_tickets(&TicketFilter {
            include_done: false,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(active_tickets.len(), 1);
    assert_eq!(active_tickets[0].id, "T-2");

    let all_tickets = db
        .list_tickets(&TicketFilter {
            include_done: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(all_tickets.len(), 2);
}

#[test]
fn test_cli_subcommands_execution() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let dir = tempdir().unwrap();
    let repo_path = dir.path();

    // 1. rig init
    let output = Command::new(rig_bin)
        .arg("init")
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig init");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Initialized rig repository"));
    assert!(repo_path.join(".rig").join("rig.db").exists());

    // 2. rig set & rig get
    let output = Command::new(rig_bin)
        .args(["set", "worker.harness", "claude"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig set");
    assert!(output.status.success());

    let output = Command::new(rig_bin)
        .args(["get", "worker.harness"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig get");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().contains("claude"));

    // 3. rig new T-1
    let output = Command::new(rig_bin)
        .args([
            "new",
            "Build Core Engine",
            "--body",
            "Goal: implement SQLite",
            "--base",
            "main",
        ])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig new T-1");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-1"));

    // 4. rig new T-2 dependent on T-1
    let output = Command::new(rig_bin)
        .args([
            "new",
            "Build UI Board",
            "--after",
            "T-1",
            "--review",
            "--manager",
            "manager:p1",
        ])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig new T-2");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-2"));

    // 5. rig ls (both open, not done)
    let output = Command::new(rig_bin)
        .arg("ls")
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig ls");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-1"));
    assert!(stdout.contains("T-2"));

    // 6. rig ls --json
    let output = Command::new(rig_bin)
        .args(["ls", "--json"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig ls --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output must be valid json");
    assert_eq!(json.as_array().unwrap().len(), 2);

    // 7. rig show T-1
    let output = Command::new(rig_bin)
        .args(["show", "T-1"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig show T-1");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-1"));
    assert!(stdout.contains("Build Core Engine"));

    // 8. rig show T-1 --json
    let output = Command::new(rig_bin)
        .args(["show", "T-1", "--json"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig show T-1 --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value =
        serde_json::from_str(&stdout).expect("show output must be valid json");
    assert_eq!(val["id"], "T-1");

    // 9. rig edit T-1 status=working
    let output = Command::new(rig_bin)
        .args(["edit", "T-1", "status=working"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig edit");
    assert!(output.status.success());

    // 10. rig say & inbox
    let output = Command::new(rig_bin)
        .args([
            "say",
            "manager:p1",
            "milestone: work underway",
            "--ticket",
            "T-1",
        ])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig say");
    assert!(output.status.success());

    let output = Command::new(rig_bin)
        .arg("inbox")
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig inbox");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("milestone: work underway"));

    // 11. rig edit T-1 status=done
    let output = Command::new(rig_bin)
        .args(["edit", "T-1", "status=done"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig edit T-1 status=done");
    assert!(output.status.success());

    // 12. rig ls hides done tickets
    let output = Command::new(rig_bin)
        .arg("ls")
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig ls");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("T-1"));
    assert!(stdout.contains("T-2"));

    // 13. rig ls --all shows done tickets
    let output = Command::new(rig_bin)
        .args(["ls", "--all"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig ls --all");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-1"));
    assert!(stdout.contains("T-2"));
}

#[test]
fn test_cli_lifecycle_subcommands() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let dir = tempdir().unwrap();
    let repo_path = dir.path();

    // 1. rig init
    let output = Command::new(rig_bin)
        .arg("init")
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig init");
    assert!(output.status.success());

    // 2. rig new T-1
    let output = Command::new(rig_bin)
        .args(["new", "Lifecycle Ticket", "--base", "main"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig new");
    assert!(output.status.success());
    let ticket_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // 3. rig launch T-1 --mode headless
    let output = Command::new(rig_bin)
        .args(["launch", ticket_id.trim(), "--mode", "headless", "--harness", "claude", "--prompt", "Hello test"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig launch");
    if !output.status.success() {
        eprintln!("launch stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("launch stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Launched run"));
    assert!(stdout.contains("PID:"));

    // 4. rig ps
    let output = Command::new(rig_bin)
        .args(["ps"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig ps");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-1"));

    // 5. rig ps --json
    let output = Command::new(rig_bin)
        .args(["ps", "--json"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig ps --json");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert!(json.as_array().unwrap().len() >= 1);

    // 6. rig stop <pid_or_run_id>
    let run_id = json[0]["run_id"].as_i64().unwrap();
    let output = Command::new(rig_bin)
        .args(["stop", &run_id.to_string(), "--force"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig stop");
    assert!(output.status.success());

    // 7. rig inbox manager:p1 --wait --timeout 200
    let output = Command::new(rig_bin)
        .args(["inbox", "manager:p1", "--wait", "--timeout", "200"])
        .current_dir(repo_path)
        .output()
        .expect("failed to execute rig inbox --wait");
    assert!(output.status.success());
}

#[test]
fn test_cli_slot_command() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");

    // 1. rig slot manager
    let output = Command::new(rig_bin)
        .args(["slot", "manager"])
        .output()
        .expect("failed to execute rig slot manager");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("manager"));
    assert!(stdout.contains("main"));

    // 2. rig slot worker:3 --json
    let output = Command::new(rig_bin)
        .args(["slot", "worker:3", "--json"])
        .output()
        .expect("failed to execute rig slot worker:3 --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value =
        serde_json::from_str(&stdout).expect("slot json output must be valid json");
    assert_eq!(val["role"], "worker:3");
    assert_eq!(val["tab_name"], "over-1");
    assert_eq!(val["slot_index"], 1);
    assert!(val["bounds"]["x"].is_number());
    assert!(val["bounds"]["w"].is_number());

    // 3. rig slot worker:0 --pane-max 4
    let output = Command::new(rig_bin)
        .args(["slot", "worker:0", "--pane-max", "4", "--json"])
        .output()
        .expect("failed to execute rig slot with custom pane-max");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(val["tab_name"], "main");
    assert_eq!(val["slot_index"], 2);
}

#[test]
fn test_cli_watch_command() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    use std::io::Write;

    // 1. rig watch with piped terminal output detecting Idle state
    let mut child = Command::new(rig_bin)
        .args(["watch", "T-1", "--lines", "3", "--harness", "claude"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn rig watch");

    child
        .stdin
        .as_mut()
        .expect("stdin available")
        .write_all(b"\x1b[32mBuild finished\x1b[0m\nReady to proceed\n\xe2\x9d\xaf ")
        .expect("write mock output");

    let output = child.wait_with_output().expect("wait on child");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Idle"));
    assert!(stdout.contains("Build finished"));
    assert!(stdout.contains("Ready to proceed"));

    // 2. rig watch with --input argument and --json
    let output = Command::new(rig_bin)
        .args([
            "watch",
            "T-1",
            "--input",
            "Compiling src/main.rs...\nRunning tests...",
            "--json",
        ])
        .output()
        .expect("failed to execute rig watch with --input");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(val["ticket"], "T-1");
    assert_eq!(val["settlement"], "Working");
    let tail = val["tail"].as_array().expect("tail is array");
    assert_eq!(tail.len(), 2);
}

#[test]
fn test_cli_worktree_and_land_lifecycle() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let repo_path = dir.path();

    // Setup git repo
    let status = Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .status()
        .expect("git init failed");
    assert!(status.success());

    let _ = Command::new("git")
        .args(["config", "user.name", "Rig Tester"])
        .current_dir(repo_path)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "rig@example.com"])
        .current_dir(repo_path)
        .status();

    let init_file = repo_path.join("README.md");
    std::fs::write(&init_file, "# Test Repo\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_path)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(repo_path)
        .status();

    let branch_out = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    let base_branch = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();

    // 1. rig init
    let output = Command::new(rig_bin)
        .arg("init")
        .current_dir(repo_path)
        .output()
        .expect("rig init failed");
    assert!(output.status.success());

    // 2. rig new
    let output = Command::new(rig_bin)
        .args(["new", "Feature X", "--base", &base_branch])
        .current_dir(repo_path)
        .output()
        .expect("rig new failed");
    assert!(output.status.success());
    let ticket_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(ticket_id, "T-1");

    // 3. rig worktree create T-1
    let output = Command::new(rig_bin)
        .args(["worktree", "create", "T-1", "--base", &base_branch])
        .current_dir(repo_path)
        .output()
        .expect("rig worktree create failed");
    assert!(output.status.success());
    let wt_path = repo_path.join(".rig").join("wt").join("T-1");
    assert!(wt_path.exists());

    // 4. rig worktree ls
    let output = Command::new(rig_bin)
        .args(["worktree", "ls"])
        .current_dir(repo_path)
        .output()
        .expect("rig worktree ls failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-1"));
    assert!(stdout.contains("rig/T-1"));

    // 5. Commit change in worktree
    let code_file = wt_path.join("feature.txt");
    std::fs::write(&code_file, "implemented feature\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "feature.txt"])
        .current_dir(&wt_path)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "add feature"])
        .current_dir(&wt_path)
        .status();

    // 6. Clean without landing and without --force must fail
    let output = Command::new(rig_bin)
        .args(["worktree", "clean", "T-1", "--base", &base_branch])
        .current_dir(repo_path)
        .output()
        .expect("rig worktree clean execution");
    assert!(!output.status.success());

    // 7. rig land T-1
    let output = Command::new(rig_bin)
        .args([
            "land",
            "T-1",
            "--message",
            "land(T-1): feature complete",
            "--base",
            &base_branch,
        ])
        .current_dir(repo_path)
        .output()
        .expect("rig land failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Landed"));

    // Check that feature.txt now exists in the main repo root
    assert!(repo_path.join("feature.txt").exists());

    // 8. rig worktree clean T-1 now succeeds
    let output = Command::new(rig_bin)
        .args(["worktree", "clean", "T-1", "--base", &base_branch])
        .current_dir(repo_path)
        .output()
        .expect("rig worktree clean failed");
    assert!(output.status.success());
    assert!(!wt_path.exists());

    // 9. rig worktree ls is empty
    let output = Command::new(rig_bin)
        .args(["worktree", "ls"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("rig/T-1"));
}

#[test]
fn test_cli_board_and_triage_commands() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let dir = tempdir().unwrap();
    let repo_path = dir.path();

    let status = Command::new("git")
        .arg("init")
        .current_dir(repo_path)
        .status()
        .expect("git init failed");
    assert!(status.success());

    let _ = Command::new("git")
        .args(["config", "user.name", "Rig Tester"])
        .current_dir(repo_path)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "rig@example.com"])
        .current_dir(repo_path)
        .status();

    let init_file = repo_path.join("README.md");
    std::fs::write(&init_file, "# Main Repo\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_path)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(repo_path)
        .status();

    let branch_out = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    let base_branch = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();

    let _ = Command::new("git")
        .args(["checkout", "-b", "feature/test-branch"])
        .current_dir(repo_path)
        .status();

    let feature_file = repo_path.join("feature.rs");
    std::fs::write(&feature_file, "pub fn feature_work() -> bool { true }\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(repo_path)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "feat: implement test branch"])
        .current_dir(repo_path)
        .status();

    let _ = Command::new("git")
        .args(["checkout", &base_branch])
        .current_dir(repo_path)
        .status();

    let output = Command::new(rig_bin)
        .arg("init")
        .current_dir(repo_path)
        .output()
        .expect("rig init failed");
    assert!(output.status.success());

    let output = Command::new(rig_bin)
        .args([
            "new",
            "Board Feature Test",
            "--base",
            &base_branch,
            "--body",
            "Testing board and triage CLI",
        ])
        .current_dir(repo_path)
        .output()
        .expect("rig new failed");
    assert!(output.status.success());
    let ticket_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(ticket_id, "T-1");

    let output = Command::new(rig_bin)
        .args(["edit", "T-1", "--branch", "feature/test-branch"])
        .current_dir(repo_path)
        .output()
        .expect("rig edit failed");
    assert!(output.status.success());

    let output = Command::new(rig_bin)
        .arg("board")
        .current_dir(repo_path)
        .output()
        .expect("rig board failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== RIG BOARD TUI ==="));
    assert!(stdout.contains("To do"));
    assert!(stdout.contains("T-1"));

    let output = Command::new(rig_bin)
        .args(["board", "--json"])
        .current_dir(repo_path)
        .output()
        .expect("rig board --json failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("rig board --json must output valid json");
    assert!(json.get("To do").is_some());
    assert!(json.get("Working").is_some());
    assert!(json.get("Ready").is_some());
    assert!(json.get("Queued").is_some());
    assert!(json.get("Needs you").is_some());
    assert!(json.get("Done").is_some());

    let todo_list = json["To do"].as_array().expect("To do should be an array");
    assert_eq!(todo_list.len(), 1);
    assert_eq!(todo_list[0]["id"], "T-1");

    let output = Command::new(rig_bin)
        .args(["board", "--watch", "--once"])
        .current_dir(repo_path)
        .output()
        .expect("rig board --watch --once failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== RIG BOARD TUI ==="));
    assert!(stdout.contains("\x1b[2J\x1b[H"));

    let output = Command::new(rig_bin)
        .args(["triage", "T-1", "--threshold", "100"])
        .current_dir(repo_path)
        .output()
        .expect("rig triage failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== TRIAGE EVALUATION ==="));
    assert!(stdout.contains("Ticket: T-1"));
    assert!(stdout.contains("Approve local"));

    let output = Command::new(rig_bin)
        .args(["triage", "T-1", "--json"])
        .current_dir(repo_path)
        .output()
        .expect("rig triage --json failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("rig triage --json must output valid json");
    assert_eq!(json["ticket_id"], "T-1");
    assert_eq!(json["branch"], "feature/test-branch");
    assert_eq!(json["base"], base_branch);
    assert!(json.get("diff_stat").is_some());
    assert_eq!(json["diff_stat"]["files_changed"], 1);
    assert!(json.get("action").is_some());
}

#[test]
fn test_cli_cross_repo_and_skill_commands() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let temp_rig_home = tempdir().unwrap();
    let temp_home_path = temp_rig_home.path();
    let rig_home_str = temp_home_path.to_string_lossy().to_string();

    // 1. rig skill manager
    let output = Command::new(rig_bin)
        .args(["skill", "manager"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig skill manager failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.to_lowercase().contains("rig manager"));

    // 2. rig skill orchestrator
    let output = Command::new(rig_bin)
        .args(["skill", "orchestrator"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig skill orchestrator failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.to_lowercase().contains("rig orchestrator"));

    // 3. rig install --home <temp_home>
    let test_home = tempdir().unwrap();
    let test_home_path = test_home.path();
    // Pre-create .claude directory to simulate installed harness
    std::fs::create_dir_all(test_home_path.join(".claude")).unwrap();

    let output = Command::new(rig_bin)
        .args(["install", "--home", &test_home_path.to_string_lossy()])
        .env("RIG_HOME", &rig_home_str)
        .env_remove("CLAUDE_CONFIG_DIR")
        .output()
        .expect("rig install failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rig-manager"));
    assert!(test_home_path
        .join(".claude")
        .join("skills")
        .join("rig-manager")
        .join("SKILL.md")
        .exists());
    assert!(test_home_path
        .join(".claude")
        .join("skills")
        .join("rig-orchestrator")
        .join("SKILL.md")
        .exists());

    // 4. rig repos add test-repo <path>
    let dummy_repo_dir = tempdir().unwrap();
    let repo_path_str = dummy_repo_dir.path().to_string_lossy().to_string();

    let output = Command::new(rig_bin)
        .args(["repos", "add", "test-repo", &repo_path_str])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig repos add failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test-repo"));

    // rig repos (list)
    let output = Command::new(rig_bin)
        .args(["repos"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig repos list failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test-repo"));

    // 5. rig story S-1 "Cross Repo Story"
    let output = Command::new(rig_bin)
        .args(["story", "S-1", "Cross Repo Story"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig story create failed");
    assert!(output.status.success());

    // rig story S-1 (get)
    let output = Command::new(rig_bin)
        .args(["story", "S-1"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig story get failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Cross Repo Story"));

    // 6. rig say orchestrator "Help needed" --story S-1
    let output = Command::new(rig_bin)
        .args(["say", "orchestrator", "Help needed", "--story", "S-1"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig say orchestrator failed");
    assert!(output.status.success());

    // 7. rig inbox --orch --story S-1
    let output = Command::new(rig_bin)
        .args(["inbox", "--orch", "--story", "S-1"])
        .env("RIG_HOME", &rig_home_str)
        .output()
        .expect("rig inbox --orch failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Help needed"));
    assert!(stdout.contains("S-1"));
}

