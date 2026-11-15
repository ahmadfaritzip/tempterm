use std::sync::Arc;
use temp_terminal_core::Registry;
use temp_terminal_ui::workspace::WorkspaceApp;
use temp_terminal_ui::{AppHost, Session, TerminalPane};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_running_shells_displays_all_four_sessions() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let tmp = std::env::temp_dir();
    host.add_project("proj_test", "TestProject", tmp).await;

    // Create 4 separate sessions with 1 terminal shell pane each
    for i in 1..=4 {
        let (tx, _rx) = mpsc::channel(32);
        let pane_id = format!("pane_{}", i);
        let session_id = format!("session_{}", i);
        let session_name = format!("terminal_{}", i);

        let pane = Arc::new(TerminalPane::new(
            pane_id.clone(),
            "terminal".to_string(),
            80,
            24,
            None,
            tx,
        ));

        let mut projs = host.projects.write().await;
        let proj = projs.get_mut("proj_test").unwrap();
        let mut sess = Session::new(session_id.clone(), session_name);
        sess.add_pane(pane);
        proj.sessions.insert(session_id.clone(), sess);
        if i == 1 {
            proj.active_session_id = session_id;
        }
    }

    let mut app = WorkspaceApp::new(host, registry, handle);
    app.refresh_snapshot();

    // Verify all 4 running shells are captured in snapshot
    assert_eq!(
        app.snapshot.running_shells.len(),
        4,
        "Expected all 4 running shells to be listed in snapshot.running_shells"
    );
    assert_eq!(
        app.snapshot.running_processes,
        4,
        "Expected running_processes count to be 4"
    );

    let pane_ids: Vec<String> = app
        .snapshot
        .running_shells
        .iter()
        .map(|s| s.pane_id.clone())
        .collect();
    assert!(pane_ids.contains(&"pane_1".to_string()));
    assert!(pane_ids.contains(&"pane_2".to_string()));
    assert!(pane_ids.contains(&"pane_3".to_string()));
    assert!(pane_ids.contains(&"pane_4".to_string()));
}

#[tokio::test]
async fn test_new_menu_and_new_project_dialog_lifecycle() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let mut app = WorkspaceApp::new(host, registry, handle);
    assert!(!app.new_menu_open);
    assert!(!app.new_project_dialog.is_open);

    // Open new menu
    app.new_menu_open = true;
    assert!(app.has_any_modal_open());

    // Switch to new project dialog
    app.new_menu_open = false;
    app.new_project_dialog.is_open = true;
    assert!(app.has_any_modal_open());

    // Close all modals
    app.close_all_modals();
    assert!(!app.new_menu_open);
    assert!(!app.new_project_dialog.is_open);
    assert!(!app.has_any_modal_open());
}

#[tokio::test]
async fn test_agent_dialog_cancel_and_start_closes_modal() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let tmp = std::env::temp_dir();
    host.add_project("proj_test", "TestProject", tmp).await;

    let mut app = WorkspaceApp::new(host, registry, handle);

    // 1. Open agent dialog
    app.agent_dialog.is_open = true;
    app.agent_dialog.selected_agent = "agy".to_string();
    assert!(app.agent_dialog.is_open);
    assert!(app.has_any_modal_open());

    // 2. Simulate Cancel button
    app.agent_dialog.is_open = false;
    assert!(!app.agent_dialog.is_open);

    // 3. Re-open and simulate Start Agent button
    app.agent_dialog.is_open = true;
    let agent = app.agent_dialog.selected_agent.clone();
    assert_eq!(agent, "agy");
    app.agent_dialog.is_open = false;
    app.trigger_new_agent(&agent, false, "", "", false);

    assert!(!app.agent_dialog.is_open);
}

#[tokio::test]
async fn test_dialogs_cancel_consistently() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let mut app = WorkspaceApp::new(host, registry, handle);

    // Process dialog
    app.process_dialog.is_open = true;
    assert!(app.has_any_modal_open());
    app.process_dialog.is_open = false;
    assert!(!app.has_any_modal_open());

    // Open Project dialog
    app.open_project_dialog.is_open = true;
    assert!(app.has_any_modal_open());
    app.open_project_dialog.is_open = false;
    assert!(!app.has_any_modal_open());

    // New Project dialog
    app.new_project_dialog.is_open = true;
    assert!(app.has_any_modal_open());
    app.new_project_dialog.is_open = false;
    assert!(!app.has_any_modal_open());
}
