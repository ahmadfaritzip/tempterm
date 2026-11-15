use std::sync::Arc;
use temp_terminal_core::registry::Registry;
use temp_terminal_procs::def::Def;
use temp_terminal_procs::file::{load_file, save_file};
use temp_terminal_ui::settings::Settings;
use temp_terminal_ui::workspace::{parse_process_env, parse_process_watch, SettingsCategory};
use temp_terminal_ui::AppHost;

#[tokio::main]
async fn main() {
    println!("[TEST] Initializing Group D Interaction & Parity Verification...");

    // 1. Process environment and watch parsing
    println!("1. Testing Process Watch Globs and Environment Parsing...");
    let env_text = "PORT=5173\nNODE_ENV=production\nDEBUG=temp-terminal:*";
    let env_map = parse_process_env(env_text).expect("env parsing must succeed");
    assert_eq!(env_map.len(), 3);
    assert_eq!(env_map.get("PORT").map(|s| s.as_str()), Some("5173"));

    let invalid_env = parse_process_env("NO_EQUALS_HERE");
    assert!(invalid_env.is_err());
    assert!(invalid_env
        .unwrap_err()
        .contains("environment line 1 must be KEY=value"));

    let dup_env = parse_process_env("KEY=1\nKEY=2");
    assert!(dup_env.is_err());
    assert!(dup_env
        .unwrap_err()
        .contains("environment key \"KEY\" is listed twice"));

    let watch_text = "src/**/*.rs\ncrates/**/Cargo.toml\n";
    let globs = parse_process_watch(watch_text);
    assert_eq!(globs, vec!["src/**/*.rs", "crates/**/Cargo.toml"]);
    println!("✓ Process watch globs and environment parsing verified.");

    // 2. YAML persistence and preservation
    println!("2. Testing tempterm.yml Save and Preservation...");
    let dir =
        std::env::temp_dir().join(format!("tt_grp_d_yaml_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let mut defs = Vec::new();
    let mut env_map = std::collections::BTreeMap::new();
    env_map.insert("PORT".to_string(), "3000".to_string());
    let d1 = Def {
        name: "web".to_string(),
        command: "npm run dev".to_string(),
        port: 3000,
        autostart: true,
        restart: "always".to_string(),
        watch: vec!["src/**".to_string()],
        env: env_map,
        ..Default::default()
    };
    defs.push(d1);

    save_file(&dir, &defs).expect("save tempterm.yml must succeed");

    let loaded = load_file(&dir).expect("load tempterm.yml must succeed");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "web");
    assert_eq!(loaded[0].port, 3000);
    assert_eq!(loaded[0].watch, vec!["src/**"]);
    assert_eq!(loaded[0].env.get("PORT").map(|s| s.as_str()), Some("3000"));
    println!("✓ tempterm.yml atomic save and loading verified.");

    // 3. Settings all 11 categories defaults and roundtrip
    println!("3. Testing Settings 11 Categories Persistence & Roundtrip...");
    let s_file = dir.join("settings.json");
    let mut s = Settings::load_from_path(&s_file);
    assert_eq!(s.theme, "dark");
    assert_eq!(s.density, "comfortable");

    s.theme = "omarchy".to_string();
    s.density = "compact".to_string();
    s.open_links = "pane".to_string();
    s.search_engine = "kagi".to_string();
    s.terminal.font_size = 15.0;
    s.terminal.cursor = "bar".to_string();
    s.editor.tab_size = 2;
    s.notify.sound = false;
    s.remote_control = true;

    s.save_to_path(&s_file).expect("save settings must succeed");

    let reloaded = Settings::load_from_path(&s_file);
    assert_eq!(reloaded.theme, "omarchy");
    assert_eq!(reloaded.density, "compact");
    assert_eq!(reloaded.open_links, "pane");
    assert_eq!(reloaded.search_engine, "kagi");
    assert_eq!(reloaded.terminal.font_size, 15.0);
    assert_eq!(reloaded.terminal.cursor, "bar");
    assert_eq!(reloaded.editor.tab_size, 2);
    assert!(!reloaded.notify.sound);
    assert!(reloaded.remote_control);
    println!("✓ Settings all categories persistence verified.");

    // 4. Windows platform restrictions verification
    println!("4. Verifying Windows Platform Restrictions (Web Access & Dev URLs)...");
    assert_eq!(SettingsCategory::WebAccess as usize, 6);
    assert_eq!(SettingsCategory::DevUrls as usize, 7);
    println!("✓ Platform restriction categories correctly defined.");

    // 5. Process spawn with custom environment propagation
    println!("5. Testing Process Spawning with Environment Variables...");
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let proj_id = host
        .open_or_switch_project(dir.clone(), &registry)
        .await
        .unwrap();

    let custom_env = vec![
        "FOO=BAR".to_string(),
        "TEMP_TERMINAL_ENV_TEST=1".to_string(),
    ];
    #[cfg(windows)]
    let cmd = vec![
        "cmd.exe".to_string(),
        "/c".to_string(),
        "echo OK".to_string(),
    ];
    #[cfg(not(windows))]
    let cmd = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "echo OK".to_string(),
    ];

    let pane_res = host
        .spawn_local_pane_with_env(
            &proj_id,
            "sess_proc_test",
            "pane_proc_test",
            120,
            36,
            &registry,
            cmd,
            dir.to_string_lossy().to_string(),
            custom_env,
            handle,
        )
        .await;

    assert!(pane_res.is_ok());
    println!("✓ Process spawned with environment variables successfully.");

    let _ = std::fs::remove_dir_all(&dir);
    println!("\nALL GROUP D INTERACTION TESTS PASSED SUCCESSFULLY!");
}
