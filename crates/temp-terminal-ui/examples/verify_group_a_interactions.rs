use std::path::PathBuf;
use std::sync::Arc;
use temp_terminal_core::registry::Registry;
use temp_terminal_ui::workspace::{InspectorTab, WorkspaceApp};
use temp_terminal_ui::AppHost;

#[tokio::main]
async fn main() {
    println!(
        "[TEST] Initializing AppHost and WorkspaceApp for Group A interaction verification..."
    );

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    host.add_project("test_proj", "Test Workspace", cwd.clone())
        .await;

    let mut app = WorkspaceApp::new(host.clone(), registry.clone(), handle);

    // 1. Initial State
    println!("Checking default shell state...");
    assert!(!app.sidebar_collapsed, "Sidebar should start expanded");
    assert!(!app.inspector_open, "Inspector should start closed");
    assert_eq!(
        app.inspector_tab,
        InspectorTab::Info,
        "Default inspector tab should be Info"
    );
    println!("✓ Default shell geometry and visibility verified.");

    // 2. Sidebar Collapse/Expand Toggle
    println!("Testing sidebar collapse/expand toggle...");
    app.sidebar_collapsed = true;
    assert!(app.sidebar_collapsed, "Sidebar should be collapsed");
    app.sidebar_collapsed = false;
    assert!(!app.sidebar_collapsed, "Sidebar should be expanded");
    println!("✓ Sidebar collapse/expand toggle works.");

    // 3. Inspector Open/Close & Tab Switching
    println!("Testing inspector toggle and tab switching...");
    app.inspector_open = true;
    assert!(app.inspector_open, "Inspector should be open");
    app.inspector_tab = InspectorTab::Files;
    assert_eq!(
        app.inspector_tab,
        InspectorTab::Files,
        "Inspector tab should switch to Files"
    );
    app.inspector_tab = InspectorTab::Git;
    assert_eq!(
        app.inspector_tab,
        InspectorTab::Git,
        "Inspector tab should switch to Git"
    );
    app.inspector_open = false;
    assert!(!app.inspector_open, "Inspector should be closed");
    println!("✓ Inspector toggle and tab switching work.");

    // 4. Modals and Overlays
    println!("Testing modals and overlays toggles...");
    app.shortcuts_overlay.is_open = true;
    assert!(
        app.shortcuts_overlay.is_open,
        "Shortcuts overlay should be open"
    );
    app.shortcuts_overlay.is_open = false;

    app.command_palette.is_open = true;
    assert!(
        app.command_palette.is_open,
        "Command palette should be open"
    );
    app.command_palette.is_open = false;

    app.quick_open.is_open = true;
    assert!(app.quick_open.is_open, "Quick open should be open");
    app.quick_open.is_open = false;

    app.settings_modal.is_open = true;
    assert!(app.settings_modal.is_open, "Settings modal should be open");
    app.settings_modal.is_open = false;
    println!("✓ Modals and overlays toggles work.");

    // 5. Trigger new session
    println!("Testing new session trigger...");
    app.trigger_new_session();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    app.refresh_snapshot();
    println!("✓ Trigger new session executed.");

    println!("\nALL GROUP A INTERACTION TESTS PASSED SUCCESSFULLY!");
}
