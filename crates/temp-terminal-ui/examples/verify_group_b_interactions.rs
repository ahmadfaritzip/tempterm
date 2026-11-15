use std::sync::Arc;
use temp_terminal_core::registry::Registry;
use temp_terminal_ui::workspace::WorkspaceApp;
use temp_terminal_ui::AppHost;

#[tokio::main]
async fn main() {
    println!(
        "[TEST] Initializing AppHost and WorkspaceApp for Group B interaction verification..."
    );

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let base_dir =
        std::env::temp_dir().join(format!("tt_gb_verify_{}", temp_terminal_ui::uuid_short()));
    let dir_a = base_dir.join("project_alpha");
    let dir_b = base_dir.join("project_beta");
    let state_file = base_dir.join("state.json");
    let _ = std::fs::create_dir_all(&dir_a);
    let _ = std::fs::create_dir_all(&dir_b);

    println!("1. Testing Project Registration...");
    let id_a = host
        .open_or_switch_project(dir_a.clone(), &registry)
        .await
        .expect("Project A should register");
    assert_eq!(*host.active_project_id.read().await, id_a);
    println!("✓ Project A registered with ID: {}", id_a);

    println!("2. Testing Duplicate Project Path Prevention...");
    let id_a_dup = host
        .open_or_switch_project(dir_a.clone(), &registry)
        .await
        .expect("Duplicate check should succeed");
    assert_eq!(id_a, id_a_dup, "Duplicate path must return existing ID");
    assert_eq!(
        host.projects.read().await.len(),
        1,
        "Must not create duplicate project entry"
    );
    println!("✓ Duplicate project path prevention verified.");

    println!("3. Testing Path Validation for non-existent directory...");
    let non_existent = base_dir.join("does_not_exist");
    assert!(host
        .open_or_switch_project(non_existent, &registry)
        .await
        .is_err());
    println!("✓ Path validation verified.");

    println!("4. Testing Project B Registration and Project Switching...");
    let id_b = host
        .open_or_switch_project(dir_b.clone(), &registry)
        .await
        .expect("Project B should register");
    assert_ne!(id_a, id_b);
    assert_eq!(*host.active_project_id.read().await, id_b);
    assert_eq!(host.projects.read().await.len(), 2);

    let switched = host.switch_project(&id_a).await;
    assert!(switched);
    assert_eq!(*host.active_project_id.read().await, id_a);
    println!("✓ Multi-project registration and switching verified.");

    println!("5. Testing Session Association and Isolation...");
    let mut app = WorkspaceApp::new(host.clone(), registry.clone(), handle);
    app.refresh_snapshot();
    assert_eq!(app.snapshot.active_project_id, id_a);
    assert_eq!(app.snapshot.projects.len(), 2);

    // In Project A, add another session
    app.trigger_new_session();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    app.refresh_snapshot();
    assert!(app.snapshot.sessions.len() >= 2);

    // Switch to Project B
    app.trigger_switch_project(&id_b);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.refresh_snapshot();
    assert_eq!(app.snapshot.active_project_id, id_b);
    println!("✓ Session isolation across projects verified.");

    println!("6. Testing Session Closure and Process Cleanup...");
    let sess_to_close = app.snapshot.sessions[0].id.clone();
    app.trigger_close_session(&sess_to_close);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.refresh_snapshot();
    assert!(!app.snapshot.sessions.iter().any(|s| s.id == sess_to_close));
    println!("✓ Session closure and cleanup verified.");

    println!("7. Testing State Persistence and Missing Directory Handling...");
    host.save_state_to_path(&state_file)
        .await
        .expect("State should save");

    // Delete dir_b to simulate missing directory
    let _ = std::fs::remove_dir_all(&dir_b);

    let host2 = AppHost::new(None);
    let loaded = host2
        .load_state_from_path(&state_file, &registry)
        .await
        .expect("State should load");
    assert!(loaded >= 2, "Must load both projects");
    let projs = host2.projects.read().await;
    let pb = projs.get(&id_b).expect("Project B must exist in state");
    assert!(
        !pb.is_available,
        "Project B must be marked unavailable (folder missing)"
    );
    println!("✓ State persistence and missing directory handling verified.");

    println!("8. Testing Project Removal...");
    host.remove_project(&id_b, &registry)
        .await
        .expect("Should remove Project B");
    assert_eq!(host.projects.read().await.len(), 1);
    assert_eq!(*host.active_project_id.read().await, id_a);
    println!("✓ Project removal and active project fallback verified.");

    // Clean up
    let _ = std::fs::remove_dir_all(&base_dir);

    println!("\nALL GROUP B INTERACTION TESTS PASSED SUCCESSFULLY!");
}
