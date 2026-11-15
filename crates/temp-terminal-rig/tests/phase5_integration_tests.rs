use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::orchestrator::OrchestratorEngine;
use temp_terminal_rig::skills::SkillsDeployer;

fn init_git_repo(path: &Path) {
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
    assert!(status.success(), "git config user.name failed");

    let status = Command::new("git")
        .args(["config", "user.email", "rig-tester@example.com"])
        .current_dir(path)
        .status()
        .expect("git config user.email failed");
    assert!(status.success(), "git config user.email failed");

    let readme = path.join("README.md");
    fs::write(&readme, "# Rig Phase 5 Integration Test Repo\n").expect("write readme");

    let status = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(path)
        .status()
        .expect("git add failed");
    assert!(status.success(), "git add failed");

    let status = Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(path)
        .status()
        .expect("git commit failed");
    assert!(status.success(), "git commit failed");
}

fn is_emoji_char(c: char) -> bool {
    matches!(
        c as u32,
        0x1F600..=0x1F64F
            | 0x1F300..=0x1F5FF
            | 0x1F680..=0x1F6FF
            | 0x1F700..=0x1F77F
            | 0x1F780..=0x1F7FF
            | 0x1F800..=0x1F8FF
            | 0x1F900..=0x1F9FF
            | 0x1FA00..=0x1FA6F
            | 0x1FA70..=0x1FAFF
            | 0x2600..=0x26FF
            | 0x2700..=0x27BF
            | 0xFE00..=0xFE0F
            | 0x1F1E6..=0x1F1FF
    )
}

#[test]
fn test_phase5_end_to_end_orchestration_and_skills() {
    // 1. Initialize isolated test environments
    let hub_tmp = tempdir().expect("create hub tempdir");
    let repo_api_tmp = tempdir().expect("create repo api tempdir");
    let repo_web_tmp = tempdir().expect("create repo web tempdir");
    let agent_home_tmp = tempdir().expect("create agent home tempdir");

    init_git_repo(repo_api_tmp.path());
    init_git_repo(repo_web_tmp.path());

    // 2. Open databases
    let hub_db = RigDatabase::open_hub(hub_tmp.path()).expect("open hub db");
    let db_api = RigDatabase::open_repo(repo_api_tmp.path()).expect("open repo api db");
    let db_web = RigDatabase::open_repo(repo_web_tmp.path()).expect("open repo web db");

    // 3. Register both repositories to hub
    hub_db
        .register_repo("repo-api", repo_api_tmp.path())
        .expect("register repo-api");
    hub_db
        .register_repo("repo-web", repo_web_tmp.path())
        .expect("register repo-web");

    let repos = hub_db.list_repos().expect("list repos from hub");
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0].name, "repo-api");
    assert_eq!(repos[1].name, "repo-web");

    // 4. Create story S-10 ("Unified Auth") and link both repositories
    let story = hub_db
        .upsert_story(
            "S-10",
            "Unified Auth",
            "Implement cross-repo authentication and token exchange protocol",
        )
        .expect("upsert story S-10");
    assert_eq!(story.id, "S-10");
    assert_eq!(story.title, "Unified Auth");
    assert_eq!(story.status, "open");

    hub_db
        .link_story_repo("S-10", "repo-api")
        .expect("link repo-api to S-10");
    hub_db
        .link_story_repo("S-10", "repo-web")
        .expect("link repo-web to S-10");

    let linked_repos = hub_db.get_story_repos("S-10").expect("get story repos");
    assert_eq!(linked_repos, vec!["repo-api", "repo-web"]);

    // 5. Deploy skills to target agent harnesses
    let installed_skills = SkillsDeployer::install_skills(
        agent_home_tmp.path(),
        Some(&["claude", "pi"]),
    )
    .expect("install skills for claude and pi");

    // 4 roles x 2 harnesses = 8 installed skill configurations
    assert_eq!(installed_skills.len(), 8);

    for result in &installed_skills {
        let skill_file = result.skill_dir.join("SKILL.md");
        assert!(
            skill_file.exists(),
            "Expected skill file to exist at {:?}",
            skill_file
        );
        let content = fs::read_to_string(&skill_file).expect("read skill file");
        assert!(content.contains(&format!("name: rig-{}", result.role.as_str())));
        assert!(content.contains(&format!("# rig {}", result.role.as_str())));
        assert!(!content.contains('\u{2014}'));
        for ch in content.chars() {
            assert!(!is_emoji_char(ch));
        }
    }

    // 6. Create tickets with story S-10 tag in both repositories
    let t1 = db_api
        .create_ticket(
            "Implement OAuth token validation",
            "REST endpoint for validating JWT bearer tokens",
            "manager",
            true,
            None,
            None,
            Some("S-10"),
        )
        .expect("create t1 in repo-api");
    assert_eq!(t1.id, "T-1");
    assert_eq!(t1.story.as_deref(), Some("S-10"));

    let mut t1_with_branch = t1.clone();
    t1_with_branch.branch = Some("feat/token-val".to_string());
    db_api
        .update_ticket(&t1_with_branch)
        .expect("update t1 branch");

    // In repo_web: create a setup ticket first so the story ticket is T-2
    let _t_setup = db_web
        .create_ticket(
            "Setup web frontend scaffold",
            "Initial setup for web workspace",
            "manager",
            false,
            None,
            None,
            None,
        )
        .expect("create setup ticket in repo-web");

    let t2 = db_web
        .create_ticket(
            "Implement login redirect and callback",
            "Handle OAuth callback and token storage in browser",
            "manager",
            true,
            None,
            None,
            Some("S-10"),
        )
        .expect("create t2 in repo-web");
    assert_eq!(t2.id, "T-2");
    assert_eq!(t2.story.as_deref(), Some("S-10"));

    let mut t2_with_branch = t2.clone();
    t2_with_branch.branch = Some("feat/login-redirect".to_string());
    db_web
        .update_ticket(&t2_with_branch)
        .expect("update t2 branch");

    // 7. Send hub message from repo-api to orchestrator
    let sent_msg = hub_db
        .send_hub_message(
            Some("T-1"),
            "repo-api/worker",
            "orchestrator",
            "Ready for cross-repo token validation contract testing",
            Some("S-10"),
        )
        .expect("send hub message");

    assert_eq!(sent_msg.frm, "repo-api/worker");
    assert_eq!(sent_msg.too, "orchestrator");
    assert_eq!(sent_msg.ticket.as_deref(), Some("T-1"));
    assert_eq!(sent_msg.story.as_deref(), Some("S-10"));

    // 8. Verify reading inbox by orchestrator with story S-10 filter
    let inbox = hub_db
        .read_hub_inbox("orchestrator", true, Some("S-10"))
        .expect("read hub inbox for orchestrator");

    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].id, sent_msg.id);
    assert_eq!(
        inbox[0].body,
        "Ready for cross-repo token validation contract testing"
    );
    assert_eq!(inbox[0].story.as_deref(), Some("S-10"));
    assert!(inbox[0].seen);

    // Reading again with mark_seen = false shows seen status persisted
    let inbox_subsequent = hub_db
        .read_hub_inbox("orchestrator", false, Some("S-10"))
        .expect("read hub inbox second time");
    assert_eq!(inbox_subsequent.len(), 1);
    assert!(inbox_subsequent[0].seen);

    // 9. Render multi-repo orchestrator board
    let board_text = OrchestratorEngine::render_orchestrator_board(&hub_db, Some("S-10"), true)
        .expect("render orchestrator board");

    assert!(board_text.contains("=== RIG ORCHESTRATOR MULTI-REPO BOARD ==="));
    assert!(board_text.contains("Story: [S-10] Unified Auth"));
    assert!(board_text.contains("[Repo: repo-api]"));
    assert!(board_text.contains("[Repo: repo-web]"));
    assert!(board_text.contains("[T-1] Implement OAuth token validation"));
    assert!(board_text.contains("branch: feat/token-val"));
    assert!(board_text.contains("[T-2] Implement login redirect and callback"));
    assert!(board_text.contains("branch: feat/login-redirect"));

    // 10. Aggregate board items and verify tickets under story S-10
    let board_items = OrchestratorEngine::aggregate_board(&hub_db, Some("S-10"), true)
        .expect("aggregate board items");

    assert_eq!(board_items.len(), 2);
    let item_api = board_items
        .iter()
        .find(|i| i.repo_name == "repo-api")
        .expect("repo-api board item");
    assert_eq!(item_api.tickets.len(), 1);
    assert_eq!(item_api.tickets[0].id, "T-1");
    assert_eq!(item_api.tickets[0].story.as_deref(), Some("S-10"));

    let item_web = board_items
        .iter()
        .find(|i| i.repo_name == "repo-web")
        .expect("repo-web board item");
    assert_eq!(item_web.tickets.len(), 1);
    assert_eq!(item_web.tickets[0].id, "T-2");
    assert_eq!(item_web.tickets[0].story.as_deref(), Some("S-10"));

    // 11. Prepare launch manager brief
    let brief_path = repo_api_tmp.path().join("PROJECT_BRIEF.md");
    fs::write(
        &brief_path,
        "# Project Brief\nDeploy unified auth across web and api services.\n",
    )
    .expect("write brief file");

    let launch_plan = OrchestratorEngine::prepare_launch_manager(
        repo_api_tmp.path(),
        &brief_path,
        Some("claude"),
        Some("claude-3-5-sonnet"),
        Some("high"),
    )
    .expect("prepare launch manager");

    assert_eq!(launch_plan.repo_path, repo_api_tmp.path());
    assert_eq!(launch_plan.brief_path, brief_path);
    assert_eq!(launch_plan.role, "manager");
    assert_eq!(launch_plan.harness, "claude");
    assert_eq!(launch_plan.model.as_deref(), Some("claude-3-5-sonnet"));
    assert_eq!(launch_plan.effort.as_deref(), Some("high"));
    assert!(launch_plan.prompt.contains("Project Brief"));
    assert!(launch_plan.prompt.contains("Cross-Repo Orchestration"));
    assert!(launch_plan.prompt.contains("rig say orchestrator"));
    assert!(launch_plan.argv.contains(&"claude".to_string()));
    assert!(launch_plan.argv.contains(&"manager".to_string()));
}

#[test]
fn test_phase5_anti_slop_compliance() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let phase5_files = [
        manifest_dir.join("src").join("skills.rs"),
        manifest_dir.join("src").join("hub.rs"),
        manifest_dir.join("src").join("orchestrator.rs"),
        manifest_dir.join("tests").join("skills_tests.rs"),
        manifest_dir.join("tests").join("hub_tests.rs"),
        manifest_dir.join("tests").join("orchestrator_tests.rs"),
        manifest_dir.join("tests").join("phase5_integration_tests.rs"),
    ];

    let em_dash_codepoint: u32 = 0x2014;

    for file_path in &phase5_files {
        assert!(
            file_path.exists(),
            "Expected Phase 5 file to exist at {:?}",
            file_path
        );

        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|err| panic!("Failed to read file {:?}: {}", file_path, err));

        assert!(
            !content.is_empty(),
            "Phase 5 file {:?} must not be empty",
            file_path
        );

        for (idx, ch) in content.chars().enumerate() {
            assert_ne!(
                ch as u32,
                em_dash_codepoint,
                "Anti-slop violation: Em dash character (U+2014) detected in {:?} at char index {}",
                file_path,
                idx
            );

            assert!(
                !is_emoji_char(ch),
                "Anti-slop violation: Decorative emoji character {:?} (U+{:04X}) detected in {:?} at char index {}",
                ch,
                ch as u32,
                file_path,
                idx
            );
        }
    }
}
