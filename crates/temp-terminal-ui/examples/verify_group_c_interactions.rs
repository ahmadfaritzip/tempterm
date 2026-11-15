use std::sync::Arc;
use temp_terminal_core::registry::Registry;
use temp_terminal_ui::workspace::WorkspaceApp;
use temp_terminal_ui::{AppHost, SplitDir, SplitNode};

#[tokio::main]
async fn main() {
    println!(
        "[TEST] Initializing AppHost and WorkspaceApp for Group C interaction verification..."
    );

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let base_dir =
        std::env::temp_dir().join(format!("tt_gc_verify_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&base_dir);

    println!("1. Testing 120px Minimum Pane Dimension and Ratio Clamping...");
    // 1000px available: 120px is 12%
    let r1 = SplitNode::clamp_ratio(5.0, 1000.0);
    assert!((r1 - 12.0).abs() < 0.01, "Expected 12.0, got {}", r1);
    let r2 = SplitNode::clamp_ratio(95.0, 1000.0);
    assert!((r2 - 88.0).abs() < 0.01, "Expected 88.0, got {}", r2);
    // 400px available: 120px is 30%
    let r3 = SplitNode::clamp_ratio(10.0, 400.0);
    assert!((r3 - 30.0).abs() < 0.01, "Expected 30.0, got {}", r3);
    let r4 = SplitNode::clamp_ratio(90.0, 400.0);
    assert!((r4 - 70.0).abs() < 0.01, "Expected 70.0, got {}", r4);
    // Small viewport (< 240px)
    let r5 = SplitNode::clamp_ratio(25.0, 200.0);
    assert_eq!(r5, 50.0);
    println!("✓ 120px minimum dimension clamping verified.");

    println!("2. Registering Project and Initial Session...");
    let proj_id = host
        .open_or_switch_project(base_dir.clone(), &registry)
        .await
        .expect("Project should register");

    let mut app = WorkspaceApp::new(host.clone(), registry.clone(), handle);
    app.refresh_snapshot();
    assert_eq!(app.snapshot.active_project_id, proj_id);
    assert_eq!(app.snapshot.panes.len(), 1);

    println!("3. Testing Vertical Split Creation and Ratio Modification...");
    app.trigger_split(SplitDir::Vertical);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    app.refresh_snapshot();
    assert_eq!(app.snapshot.panes.len(), 2);
    assert!(app.snapshot.layout.is_some());
    assert!(!app.snapshot.layout.as_ref().unwrap().is_leaf());

    // Update vertical split ratio to 65%
    app.trigger_update_split_ratio("", 65.0);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.refresh_snapshot();
    assert_eq!(
        app.snapshot.layout.as_ref().unwrap().get_ratio_at_path(""),
        Some(65.0)
    );
    println!("✓ Vertical split ratio modification verified.");

    println!("4. Testing Nested Horizontal Split and Ratio Isolation...");
    app.trigger_split(SplitDir::Horizontal);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    app.refresh_snapshot();
    assert_eq!(app.snapshot.panes.len(), 3);

    // Update nested ratio at "b" to 40%
    app.trigger_update_split_ratio("b", 40.0);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.refresh_snapshot();
    assert_eq!(
        app.snapshot.layout.as_ref().unwrap().get_ratio_at_path("b"),
        Some(40.0)
    );
    // Root ratio must remain unchanged (65.0)
    assert_eq!(
        app.snapshot.layout.as_ref().unwrap().get_ratio_at_path(""),
        Some(65.0)
    );
    println!("✓ Nested split ratio independence verified.");

    println!("5. Testing Pane Zoom / Maximize Toggle (Ctrl+Shift+Z)...");
    assert!(app.snapshot.zoomed_pane_id.is_none());
    let active_pane = app.snapshot.active_pane_id.clone();
    assert!(!active_pane.is_empty());

    // Toggle zoom on
    app.trigger_toggle_zoom();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.refresh_snapshot();
    assert_eq!(app.snapshot.zoomed_pane_id, Some(active_pane.clone()));
    // Split tree layout is preserved while zoomed
    assert_eq!(app.snapshot.layout.as_ref().unwrap().term_ids().len(), 3);
    println!("✓ Pane zoom toggle ON verified.");

    // Toggle zoom off (restore)
    app.trigger_toggle_zoom();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.refresh_snapshot();
    assert!(app.snapshot.zoomed_pane_id.is_none());
    assert_eq!(app.snapshot.layout.as_ref().unwrap().term_ids().len(), 3);
    println!("✓ Pane zoom restore verified.");

    println!("6. Testing Pane Close While Zoomed...");
    // Zoom active pane again
    app.trigger_toggle_zoom();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.refresh_snapshot();
    assert!(app.snapshot.zoomed_pane_id.is_some());

    // Close the zoomed pane
    let pane_to_close = app.snapshot.zoomed_pane_id.clone().unwrap();
    app.trigger_close_pane(&pane_to_close);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    app.refresh_snapshot();

    // Zoom must be automatically cleared, remaining 2 panes intact
    assert!(app.snapshot.zoomed_pane_id.is_none());
    assert_eq!(app.snapshot.panes.len(), 2);
    assert!(!app.snapshot.active_pane_id.is_empty());
    println!("✓ Pane close while zoomed handled cleanly.");

    // Clean up
    let _ = std::fs::remove_dir_all(&base_dir);

    println!("\nALL GROUP C INTERACTION TESTS PASSED SUCCESSFULLY!");
}
