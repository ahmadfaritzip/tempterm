use std::fs;
use std::process::Command;
use tempfile::tempdir;

use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::orchestrator::OrchestratorEngine;
use temp_terminal_rig::ticket::TicketStatus;

#[test]
fn test_aggregate_board_multi_repo() {
    let hub_tmp = tempdir().expect("create hub tempdir");
    let hub_db = RigDatabase::open_hub(hub_tmp.path()).expect("open hub db");

    let repo_a_tmp = tempdir().expect("create repo a tempdir");
    let repo_b_tmp = tempdir().expect("create repo b tempdir");

    let repo_a_db = RigDatabase::open_repo(repo_a_tmp.path()).expect("open repo a");
    let repo_b_db = RigDatabase::open_repo(repo_b_tmp.path()).expect("open repo b");

    hub_db
        .register_repo("repo-a", repo_a_tmp.path())
        .expect("register repo a");
    hub_db
        .register_repo("repo-b", repo_b_tmp.path())
        .expect("register repo b");

    let t1 = repo_a_db
        .create_ticket(
            "Setup auth schema",
            "Initial schema for auth",
            "manager",
            false,
            None,
            None,
            None,
        )
        .expect("create t1 in repo a");

    let t2 = repo_a_db
        .create_ticket(
            "Implement login endpoint",
            "REST endpoint for auth",
            "manager",
            true,
            None,
            None,
            None,
        )
        .expect("create t2 in repo a");

    repo_a_db
        .update_ticket_status(&t2.id, TicketStatus::Done)
        .expect("update t2 status to done");

    let tb1 = repo_b_db
        .create_ticket(
            "Setup web frontend",
            "Initial frontend layout",
            "manager",
            false,
            None,
            None,
            None,
        )
        .expect("create tb1 in repo b");

    // Aggregate with include_done = false
    let items = OrchestratorEngine::aggregate_board(&hub_db, None, false)
        .expect("aggregate board without done");

    assert_eq!(items.len(), 2);
    let item_a = items
        .iter()
        .find(|item| item.repo_name == "repo-a")
        .expect("repo-a item found");
    let item_b = items
        .iter()
        .find(|item| item.repo_name == "repo-b")
        .expect("repo-b item found");

    assert_eq!(item_a.tickets.len(), 1);
    assert_eq!(item_a.tickets[0].id, t1.id);

    assert_eq!(item_b.tickets.len(), 1);
    assert_eq!(item_b.tickets[0].id, tb1.id);

    // Aggregate with include_done = true
    let items_with_done = OrchestratorEngine::aggregate_board(&hub_db, None, true)
        .expect("aggregate board with done");

    let item_a_done = items_with_done
        .iter()
        .find(|item| item.repo_name == "repo-a")
        .expect("repo-a item found");
    assert_eq!(item_a_done.tickets.len(), 2);
}

#[test]
fn test_aggregate_board_with_story_filter() {
    let hub_tmp = tempdir().expect("create hub tempdir");
    let hub_db = RigDatabase::open_hub(hub_tmp.path()).expect("open hub db");

    let repo_a_tmp = tempdir().expect("create repo a tempdir");
    let repo_b_tmp = tempdir().expect("create repo b tempdir");

    let repo_a_db = RigDatabase::open_repo(repo_a_tmp.path()).expect("open repo a");
    let repo_b_db = RigDatabase::open_repo(repo_b_tmp.path()).expect("open repo b");

    hub_db
        .register_repo("api-service", repo_a_tmp.path())
        .expect("register api");
    hub_db
        .register_repo("web-service", repo_b_tmp.path())
        .expect("register web");

    let _t1 = repo_a_db
        .create_ticket(
            "API Auth",
            "Story S-100 task",
            "manager",
            false,
            None,
            None,
            Some("S-100"),
        )
        .expect("create t1");

    let _t2 = repo_a_db
        .create_ticket(
            "API Unrelated",
            "Story S-200 task",
            "manager",
            false,
            None,
            None,
            Some("S-200"),
        )
        .expect("create t2");

    let _t3 = repo_b_db
        .create_ticket(
            "Web Auth",
            "Story S-100 frontend",
            "manager",
            false,
            None,
            None,
            Some("S-100"),
        )
        .expect("create t3");

    let items = OrchestratorEngine::aggregate_board(&hub_db, Some("S-100"), true)
        .expect("aggregate with story filter");

    assert_eq!(items.len(), 2);
    for item in &items {
        assert_eq!(item.story_id.as_deref(), Some("S-100"));
        for t in &item.tickets {
            assert_eq!(t.story.as_deref(), Some("S-100"));
        }
    }

    let api_item = items
        .iter()
        .find(|i| i.repo_name == "api-service")
        .expect("api item");
    assert_eq!(api_item.tickets.len(), 1);
    assert_eq!(api_item.tickets[0].title, "API Auth");

    let web_item = items
        .iter()
        .find(|i| i.repo_name == "web-service")
        .expect("web item");
    assert_eq!(web_item.tickets.len(), 1);
    assert_eq!(web_item.tickets[0].title, "Web Auth");
}

#[test]
fn test_render_orchestrator_board() {
    let hub_tmp = tempdir().expect("create hub tempdir");
    let hub_db = RigDatabase::open_hub(hub_tmp.path()).expect("open hub db");

    let repo_tmp = tempdir().expect("create repo tempdir");
    let repo_db = RigDatabase::open_repo(repo_tmp.path()).expect("open repo db");

    hub_db
        .register_repo("billing-core", repo_tmp.path())
        .expect("register repo");
    hub_db
        .upsert_story(
            "S-42",
            "Stripe Integration",
            "Integrate Stripe billing webhooks",
        )
        .expect("upsert story");

    let mut ticket = repo_db
        .create_ticket(
            "Handle invoice payment",
            "Webhook listener",
            "manager",
            false,
            None,
            None,
            Some("S-42"),
        )
        .expect("create ticket");

    ticket.branch = Some("feat/stripe-webhooks".to_string());
    repo_db.update_ticket(&ticket).expect("update ticket branch");

    let board_text = OrchestratorEngine::render_orchestrator_board(&hub_db, Some("S-42"), false)
        .expect("render orchestrator board");

    assert!(board_text.contains("=== RIG ORCHESTRATOR MULTI-REPO BOARD ==="));
    assert!(board_text.contains("Story: [S-42] Stripe Integration"));
    assert!(board_text.contains("[Repo: billing-core]"));
    assert!(board_text.contains("[T-1] Handle invoice payment | branch: feat/stripe-webhooks | status: open"));
}

#[test]
fn test_prepare_launch_manager() {
    let repo_tmp = tempdir().expect("create repo tempdir");
    let brief_tmp = tempdir().expect("create brief tempdir");
    let brief_path = brief_tmp.path().join("PROJECT_BRIEF.md");

    fs::write(
        &brief_path,
        "Build cross-repository unified billing system and landing page.",
    )
    .expect("write brief");

    let plan = OrchestratorEngine::prepare_launch_manager(
        repo_tmp.path(),
        &brief_path,
        Some("claude"),
        Some("claude-3-5-sonnet"),
        Some("high"),
    )
    .expect("prepare launch manager");

    assert_eq!(plan.repo_path, repo_tmp.path());
    assert_eq!(plan.brief_path, brief_path);
    assert_eq!(plan.harness, "claude");
    assert_eq!(plan.model.as_deref(), Some("claude-3-5-sonnet"));
    assert_eq!(plan.effort.as_deref(), Some("high"));
    assert!(plan.prompt.contains("Build cross-repository unified billing system and landing page."));
    assert!(plan.prompt.contains("rig say orchestrator"));
    assert!(plan.argv.contains(&"claude".to_string()));
    assert!(plan.argv.contains(&"manager".to_string()));
}

#[test]
fn test_cli_board_orch_text_and_json() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let hub_tmp = tempdir().expect("create hub tempdir");
    let hub_path_str = hub_tmp.path().to_string_lossy().to_string();

    let repo_tmp = tempdir().expect("create repo tempdir");
    let repo_db = RigDatabase::open_repo(repo_tmp.path()).expect("open repo db");

    let hub_db = RigDatabase::open_hub(hub_tmp.path()).expect("open hub db");
    hub_db
        .register_repo("cli-repo", repo_tmp.path())
        .expect("register repo");
    hub_db
        .upsert_story("S-99", "Auth Flow", "Cross repo auth")
        .expect("upsert story");

    repo_db
        .create_ticket(
            "CLI Auth Task",
            "Body",
            "manager",
            false,
            None,
            None,
            Some("S-99"),
        )
        .expect("create ticket");

    // Test rig board --orch
    let output = Command::new(rig_bin)
        .args(["board", "--orch", "--story", "S-99"])
        .env("RIG_HOME", &hub_path_str)
        .output()
        .expect("execute rig board --orch");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("RIG ORCHESTRATOR MULTI-REPO BOARD"));
    assert!(stdout.contains("Story: [S-99] Auth Flow"));
    assert!(stdout.contains("[Repo: cli-repo]"));
    assert!(stdout.contains("CLI Auth Task"));

    // Test rig board --orch --json
    let json_output = Command::new(rig_bin)
        .args(["board", "--orch", "--story", "S-99", "--json"])
        .env("RIG_HOME", &hub_path_str)
        .output()
        .expect("execute rig board --orch --json");

    assert!(json_output.status.success());
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    assert!(json_stdout.contains("\"repo_name\": \"cli-repo\""));
    assert!(json_stdout.contains("\"story_id\": \"S-99\""));
}

#[test]
fn test_cli_launch_manager_brief() {
    let rig_bin = env!("CARGO_BIN_EXE_rig");
    let repo_tmp = tempdir().expect("create repo tempdir");
    let brief_tmp = tempdir().expect("create brief tempdir");
    let brief_path = brief_tmp.path().join("MANAGER_BRIEF.md");

    fs::write(&brief_path, "Deliver the authentication service MVP.").expect("write brief");

    let output = Command::new(rig_bin)
        .args([
            "launch",
            &repo_tmp.path().to_string_lossy(),
            "--brief",
            &brief_path.to_string_lossy(),
            "--harness",
            "claude",
            "--model",
            "claude-3-5-sonnet",
        ])
        .output()
        .expect("execute rig launch with brief");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Prepared launch manager for repo"));
    assert!(stdout.contains("Harness: claude"));
    assert!(stdout.contains("Model: claude-3-5-sonnet"));
    assert!(stdout.contains("Command:"));
}
