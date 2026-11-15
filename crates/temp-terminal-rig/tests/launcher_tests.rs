use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::run::RunMode;
use temp_terminal_rig::layout::{LayoutConfig, RectBounds, SlotRole, LayoutAllocator};
use temp_terminal_rig::launcher::{LaunchRequest, PreparedLaunch, ProcessLauncher};
use std::time::Duration;

#[test]
fn test_prepare_launch_defaults_and_expansion() {
    let mut db = RigDatabase::open_in_memory().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();

    let ticket = db.create_ticket(
        "Implement launcher",
        "Body content",
        "claude",
        false,
        None,
        None,
        None,
    ).unwrap();

    let request = LaunchRequest {
        ticket_id: ticket.id.clone(),
        role: SlotRole::Worker(1),
        harness: None,
        mode: None,
        resume: false,
        prompt: Some("Fix bug".to_string()),
    };

    let layout_config = LayoutConfig::default();

    let prepared = ProcessLauncher::prepare_launch(&mut db, repo_root, &request, &layout_config).unwrap();

    assert_eq!(prepared.ticket_id, ticket.id);
    assert_eq!(prepared.harness, "claude");
    assert_eq!(prepared.mode, RunMode::Pane);
    assert_eq!(prepared.role, SlotRole::Worker(1));
    assert!(prepared.run_id > 0);
    assert!(!prepared.argv.is_empty());
    assert_eq!(prepared.slot_location, LayoutAllocator::allocate_slot(&SlotRole::Worker(1), &layout_config));
}

#[test]
fn test_spawn_headless_process() {
    let temp_dir = tempfile::tempdir().unwrap();
    let log_path = temp_dir.path().join("spawn.log");

    let prepared = PreparedLaunch {
        ticket_id: "T-2".to_string(),
        run_id: 1,
        role: SlotRole::Worker(1),
        harness: "claude".to_string(),
        mode: RunMode::Pane,
        working_dir: temp_dir.path().to_path_buf(),
        #[cfg(target_os = "windows")]
        argv: vec!["cmd".to_string(), "/c".to_string(), "echo hello_rig".to_string()],
        #[cfg(not(target_os = "windows"))]
        argv: vec!["echo".to_string(), "hello_rig".to_string()],
        slot_location: LayoutAllocator::allocate_slot(&SlotRole::Worker(1), &LayoutConfig::default()),
        bounds: RectBounds { x: 0.0, y: 0.0, w: 80.0, h: 24.0 },
    };

    let pid = ProcessLauncher::spawn_headless(&prepared, Some(&log_path)).unwrap();
    assert!(pid > 0);

    std::thread::sleep(Duration::from_millis(300));
}
