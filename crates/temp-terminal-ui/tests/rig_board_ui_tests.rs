use std::path::PathBuf;
use temp_terminal_rig::{RigDatabase, TicketStatus};
use temp_terminal_ui::rig_board::{RigBoardPanel, RigBoardState};

#[test]
fn test_rig_board_state_initialization() {
    let repo_path = PathBuf::from("D:/dummy/repo");
    let state = RigBoardState::new(repo_path.clone());

    assert_eq!(state.repo_path, repo_path);
    assert!(!state.show_done, "show_done should default to false");
    assert_eq!(
        state.expanded_groups,
        [true, true, true, false, false, false],
        "NeedsYou, Working, Ready must be true by default, others false"
    );
    assert_eq!(state.selected_ticket, None);
    assert!(state.pending_toasts.is_empty());
    assert_eq!(state.action_feedback, None);
}

#[test]
fn test_select_ticket_and_toggle_group() {
    let mut state = RigBoardState::new(PathBuf::from("D:/dummy/repo"));

    // Select ticket
    state.select_ticket(Some("T-1".to_string()));
    assert_eq!(state.selected_ticket.as_deref(), Some("T-1"));

    state.select_ticket(None);
    assert_eq!(state.selected_ticket, None);

    // Toggle group within bounds
    assert!(state.expanded_groups[0]);
    state.toggle_group(0);
    assert!(!state.expanded_groups[0]);
    state.toggle_group(0);
    assert!(state.expanded_groups[0]);

    assert!(!state.expanded_groups[3]);
    state.toggle_group(3);
    assert!(state.expanded_groups[3]);

    // Out of bounds toggle should not panic
    state.toggle_group(10);
    state.toggle_group(999);
}

#[test]
fn test_check_alerts_and_pop_toast() {
    let mut db = RigDatabase::open_in_memory().expect("open in memory db");
    let mut state = RigBoardState::new(PathBuf::from("D:/dummy/repo"));

    // Empty DB should not create toasts
    state.check_alerts(&mut db);
    assert!(state.pending_toasts.is_empty());

    // Create a NeedsYou ticket
    let t1 = db
        .create_ticket("Prompt Review", "Needs user answer", "claude", false, None, None, None)
        .expect("create ticket 1");
    db.update_ticket_status(&t1.id, TicketStatus::NeedsYou)
        .expect("update status to NeedsYou");

    // Create a Blocked ticket
    let t2 = db
        .create_ticket("Blocked Task", "Needs dependency", "codex", false, None, None, None)
        .expect("create ticket 2");
    db.update_ticket_status(&t2.id, TicketStatus::Blocked)
        .expect("update status to Blocked");

    // Create a normal Working ticket
    let t3 = db
        .create_ticket("Active Task", "Working smoothly", "claude", false, None, None, None)
        .expect("create ticket 3");
    db.update_ticket_status(&t3.id, TicketStatus::Working)
        .expect("update status to Working");

    state.check_alerts(&mut db);
    assert_eq!(state.pending_toasts.len(), 2, "Expected 2 alert toasts");
    assert!(state.pending_toasts.iter().any(|t| t.contains(&t1.id)));
    assert!(state.pending_toasts.iter().any(|t| t.contains(&t2.id)));

    // Duplicate check: calling check_alerts again should not duplicate existing alerts
    state.check_alerts(&mut db);
    assert_eq!(
        state.pending_toasts.len(),
        2,
        "Duplicate check failed: alerts should not be duplicated"
    );

    // Pop toasts
    let popped1 = state.pop_toast();
    assert!(popped1.is_some());
    assert_eq!(state.pending_toasts.len(), 1);

    let popped2 = state.pop_toast();
    assert!(popped2.is_some());
    assert_eq!(state.pending_toasts.len(), 0);

    let popped_none = state.pop_toast();
    assert!(popped_none.is_none());
}

#[test]
fn test_render_rig_board_panel_minimal() {
    let mut db = RigDatabase::open_in_memory().expect("open in memory db");
    let mut state = RigBoardState::new(PathBuf::from("D:/dummy/repo"));

    // Seed several tickets across states
    let t1 = db
        .create_ticket("Fix login", "Fix auth issue", "claude", false, None, None, None)
        .expect("create t1");
    db.update_ticket_status(&t1.id, TicketStatus::NeedsYou)
        .expect("set t1 NeedsYou");

    let t2 = db
        .create_ticket("Implement search", "Add search box", "codex", false, None, None, None)
        .expect("create t2");
    db.update_ticket_status(&t2.id, TicketStatus::Working)
        .expect("set t2 Working");

    let t3 = db
        .create_ticket("Code review", "Review PR #42", "pi", true, None, None, None)
        .expect("create t3");
    db.update_ticket_status(&t3.id, TicketStatus::Review)
        .expect("set t3 Review");

    let t4 = db
        .create_ticket("Completed ticket", "All done", "claude", false, None, None, None)
        .expect("create t4");
    db.update_ticket_status(&t4.id, TicketStatus::Done)
        .expect("set t4 Done");

    state.selected_ticket = Some(t1.id.clone());
    state.pending_toasts.push("T-1 Needs user attention".to_string());

    let ctx = eframe::egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            RigBoardPanel::ui(ui, &mut state, &mut db);
        });
    });

    // Also test with show_done = true
    state.show_done = true;
    let _ = ctx.run(Default::default(), |ctx| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            RigBoardPanel::ui(ui, &mut state, &mut db);
        });
    });
}

#[test]
fn test_rig_board_no_em_dash_or_emoji() {
    // Read source code of rig_board.rs to verify rule compliance
    let content = include_str!("../src/rig_board.rs");

    assert!(
        !content.contains('\u{2014}'),
        "rig_board.rs must not contain em dash (\\u2014)"
    );

    for (line_idx, line) in content.lines().enumerate() {
        for ch in line.chars() {
            let u = ch as u32;
            let is_emoji = (0x1F300..=0x1F9FF).contains(&u)
                || (0x2600..=0x26FF).contains(&u)
                || (0x2700..=0x27BF).contains(&u);
            assert!(
                !is_emoji,
                "rig_board.rs contains decorative emoji '{}' on line {}",
                ch,
                line_idx + 1
            );
        }
    }
}

#[tokio::test]
async fn test_workspace_app_rig_tab_integration() {
    use std::sync::Arc;
    use temp_terminal_core::registry::Registry;
    use temp_terminal_ui::workspace::{InspectorTab, WorkspaceApp};
    use temp_terminal_ui::AppHost;

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();
    let mut app = WorkspaceApp::new(host, registry, handle);

    // Initial state
    assert_eq!(app.inspector_tab, InspectorTab::Info);
    assert!(!app.rig_board_state.show_done);
    assert_eq!(
        app.rig_board_state.expanded_groups,
        [true, true, true, false, false, false]
    );

    // Switch inspector tab to Rig directly
    app.inspector_tab = InspectorTab::Rig;
    assert_eq!(app.inspector_tab, InspectorTab::Rig);

    // Trigger command palette view.rig command
    app.inspector_tab = InspectorTab::Info;
    app.inspector_open = false;
    app.execute_command("view.rig");
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Rig);

    // Render rig tab in egui context
    let ctx = eframe::egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            app.render_rig_tab(ui);
        });
    });
}

