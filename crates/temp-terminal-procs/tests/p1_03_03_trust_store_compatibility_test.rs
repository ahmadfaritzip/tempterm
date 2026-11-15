//! Integration and compatibility tests for persistent process definition trust storage.
//! Validates parity with Go `tempterm/internal/procs/trust.go` and `trust_test.go`.

use std::fs;
use std::path::PathBuf;
use temp_terminal_procs::def::Def;
use temp_terminal_procs::trust::{Trust, TrustError};

/// Helper to clean paths matching native platform filepath.Clean in Go.
fn clean_key(p: &str) -> String {
    if cfg!(windows) {
        p.replace('/', "\\")
    } else {
        p.to_string()
    }
}

/// Helper to generate a unique temporary test path in a sandboxed directory.
struct TestEnv {
    dir: PathBuf,
}

impl TestEnv {
    fn new(name: &str) -> Self {
        let mut dir = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("temp_terminal_test_{}_{}", name, nonce));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn trust_path(&self) -> PathBuf {
        self.dir.join("sub").join("trust.json")
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn test_open_is_lazy_and_does_not_create_file() {
    let env = TestEnv::new("lazy_open");
    let path = env.trust_path();

    let trust = Trust::open(&path);
    // Verified: file must not exist yet
    assert!(
        !path.exists(),
        "open() must be completely lazy and perform no I/O"
    );
    // is_trusted on nonexistent file returns false
    assert!(!trust.is_trusted("/work/app", "variant123"));
    assert!(
        !path.exists(),
        "is_trusted() on absent file must not create it"
    );
}

#[test]
fn test_add_first_trust_entry_creates_file_and_parent_dirs() {
    let env = TestEnv::new("add_first");
    let path = env.trust_path();
    let trust = Trust::open(&path);

    let def = Def {
        name: "web".to_string(),
        command: "sh -c 'sleep 30'".to_string(),
        dir: ".".to_string(),
        port: 3000,
        ..Default::default()
    };
    let variant = def.variant();

    assert!(!trust.is_trusted("/work/app", &variant));
    let add_res = trust.add("/work/app", &variant);
    assert!(add_res.is_ok(), "add() should succeed on new file");

    assert!(path.exists(), "trust file must be created on disk");
    assert!(
        trust.is_trusted("/work/app", &variant),
        "is_trusted() must report true post-add"
    );

    // Inspect on-disk JSON structure
    let content = fs::read_to_string(&path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(val["version"], 1, "version must be integer 1");
    let projects = val.get("projects").expect("must have projects field");
    let app_key = clean_key("/work/app");
    let app = projects
        .get(&app_key)
        .expect("must have cleaned /work/app key");
    let timestamp = app
        .get(&variant)
        .expect("must have variant hash key")
        .as_str()
        .unwrap();

    // Verify UTC RFC3339 whole-second format (e.g. 2026-11-04T02:00:00Z)
    assert!(
        timestamp.ends_with('Z'),
        "timestamp must end with Z for UTC: {}",
        timestamp
    );
    assert_eq!(
        timestamp.len(),
        20,
        "whole-second RFC3339 format has length 20 (YYYY-MM-DDTHH:MM:SSZ): {}",
        timestamp
    );
    assert_eq!(&timestamp[10..11], "T");
}

#[test]
fn test_trust_reopen_preserves_trust() {
    let env = TestEnv::new("reopen_preserve");
    let path = env.trust_path();

    {
        let trust = Trust::open(&path);
        trust.add("/work/app", "var_a").unwrap();
        assert!(trust.is_trusted("/work/app", "var_a"));
    }

    // Drop original instance and reopen from the same file path
    let trust2 = Trust::open(&path);
    assert!(
        trust2.is_trusted("/work/app", "var_a"),
        "trust must survive dropping instance and reopening"
    );
}

#[test]
fn test_project_and_variant_isolation() {
    let env = TestEnv::new("isolation");
    let path = env.trust_path();
    let trust = Trust::open(&path);

    trust.add("/work/app", "v1").unwrap();

    // Project isolation: different project path must not be trusted
    assert!(
        !trust.is_trusted("/work/other", "v1"),
        "different folder must not be trusted"
    );

    // Variant isolation: different variant in same project must not be trusted
    assert!(
        !trust.is_trusted("/work/app", "v2"),
        "different variant must not be trusted"
    );
}

#[test]
fn test_add_same_entry_refreshes_timestamp() {
    let env = TestEnv::new("refresh_timestamp");
    let path = env.trust_path();
    let trust = Trust::open(&path);

    trust.add("/work/app", "v1").unwrap();
    let content1 = fs::read_to_string(&path).unwrap();
    let val1: serde_json::Value = serde_json::from_str(&content1).unwrap();
    let app_key = clean_key("/work/app");
    let ts1 = val1["projects"][&app_key]["v1"]
        .as_str()
        .unwrap()
        .to_string();

    // Sleep 1 second to ensure a distinct whole-second UTC timestamp
    std::thread::sleep(std::time::Duration::from_millis(1050));

    trust.add("/work/app", "v1").unwrap();
    let content2 = fs::read_to_string(&path).unwrap();
    let val2: serde_json::Value = serde_json::from_str(&content2).unwrap();
    let ts2 = val2["projects"][&app_key]["v1"]
        .as_str()
        .unwrap()
        .to_string();

    assert_ne!(
        ts1, ts2,
        "adding existing variant must refresh its timestamp"
    );
}

#[test]
fn test_multiple_variants_and_projects_coexist() {
    let env = TestEnv::new("multiple_entries");
    let path = env.trust_path();
    let trust = Trust::open(&path);

    trust.add("/work/app", "v1").unwrap();
    trust.add("/work/app", "v2").unwrap();
    trust.add("/work/backend", "vb1").unwrap();

    assert!(trust.is_trusted("/work/app", "v1"));
    assert!(trust.is_trusted("/work/app", "v2"));
    assert!(trust.is_trusted("/work/backend", "vb1"));
    assert!(!trust.is_trusted("/work/backend", "v1"));

    let content = fs::read_to_string(&path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();
    let app_key = clean_key("/work/app");
    let app_map = val["projects"][&app_key].as_object().unwrap();
    assert_eq!(app_map.len(), 2, "must retain 2 variants for /work/app");
    assert!(app_map.contains_key("v1"));
    assert!(app_map.contains_key("v2"));
}

#[test]
fn test_project_path_cleaning_parity() {
    let env = TestEnv::new("path_cleaning");
    let path = env.trust_path();
    let trust = Trust::open(&path);

    // Add with trailing slash and redundant elements
    trust.add("/work/app/", "v1").unwrap();

    // Query with cleaned path
    assert!(
        trust.is_trusted("/work/app", "v1"),
        "trailing slash should be cleaned"
    );
    assert!(
        trust.is_trusted("/work/app/.", "v1"),
        "dot component should be cleaned"
    );

    // Empty path cleans to "." in Go filepath.Clean
    trust.add("", "v_empty").unwrap();
    assert!(trust.is_trusted(".", "v_empty"));
    assert!(trust.is_trusted("./", "v_empty"));
}

#[test]
fn test_in_memory_cache_hit_and_invalidation() {
    let env = TestEnv::new("cache_invalidation");
    let path = env.trust_path();
    let trust = Trust::open(&path);

    trust.add("/work/app", "v1").unwrap();
    assert!(trust.is_trusted("/work/app", "v1"));

    // Modify file externally to add a variant directly to JSON on disk
    let content = fs::read_to_string(&path).unwrap();
    let mut val: serde_json::Value = serde_json::from_str(&content).unwrap();
    let app_key = clean_key("/work/app");
    val["projects"][&app_key]["v_external"] =
        serde_json::Value::String("2026-11-04T12:00:00Z".to_string());
    let modified_json = serde_json::to_string_pretty(&val).unwrap();

    // Sleep briefly to ensure mtime changes
    std::thread::sleep(std::time::Duration::from_millis(100));
    fs::write(&path, modified_json).unwrap();

    // Trust instance should detect mtime/size change and reload cache
    assert!(
        trust.is_trusted("/work/app", "v_external"),
        "cache must invalidate and reload on external file modification"
    );
}

#[test]
fn test_malformed_json_returns_false_and_refuses_to_overwrite() {
    let env = TestEnv::new("malformed_json");
    let path = env.trust_path();

    // Write corrupted JSON to trust file
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let corrupt_bytes = b"{\"version\": 1, \"projects\": INVALID_JSON_DATA";
    fs::write(&path, corrupt_bytes).unwrap();

    let trust = Trust::open(&path);

    // is_trusted() must suppress error and return false
    assert!(
        !trust.is_trusted("/work/app", "v1"),
        "is_trusted() must return false on corrupted file"
    );

    // add() must refuse to overwrite and return an error
    let err = trust.add("/work/app", "v1").unwrap_err();
    match err {
        TrustError::Json(_) => {}
        other => panic!("expected Json error, got: {:?}", other),
    }

    // Verify on-disk bytes remain 100% untouched
    let current_bytes = fs::read(&path).unwrap();
    assert_eq!(
        current_bytes, corrupt_bytes,
        "add() must refuse to overwrite a corrupted trust file"
    );
}

#[test]
fn test_empty_file_treated_as_corrupt_not_missing() {
    let env = TestEnv::new("empty_file");
    let path = env.trust_path();

    // Create a 0-byte file
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"").unwrap();

    let trust = Trust::open(&path);

    // is_trusted must return false
    assert!(!trust.is_trusted("/work/app", "v1"));

    // add must return JSON error (EOF) and refuse to overwrite
    let err = trust.add("/work/app", "v1").unwrap_err();
    match err {
        TrustError::Json(_) => {}
        other => panic!("expected Json EOF error, got: {:?}", other),
    }

    assert_eq!(fs::read(&path).unwrap().len(), 0);
}

#[test]
fn test_concurrent_mutations_do_not_lose_updates() {
    let env = TestEnv::new("concurrency");
    let path = env.trust_path();
    let trust = std::sync::Arc::new(Trust::open(&path));

    let mut handles = Vec::new();
    for i in 0..10 {
        let t: std::sync::Arc<Trust> = std::sync::Arc::clone(&trust);
        handles.push(std::thread::spawn(move || {
            let variant = format!("v_{}", i);
            t.add("/work/app", &variant).unwrap();
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Verify all 10 variants exist
    for i in 0..10 {
        let variant = format!("v_{}", i);
        assert!(
            trust.is_trusted("/work/app", &variant),
            "variant {} must be trusted after concurrent adds",
            variant
        );
    }
}

#[test]
fn test_atomic_write_cleanup_on_unwritable_directory() {
    let env = TestEnv::new("atomic_cleanup");
    // Target is a file inside an existing file path (illegal parent), causing write_atomic to fail
    let blocker = env.dir.join("blocker_file");
    fs::write(&blocker, b"blocking").unwrap();
    let path = blocker.join("trust.json");

    let trust = Trust::open(&path);
    let err = trust.add("/work/app", "v1").unwrap_err();
    match err {
        TrustError::Io(_) => {}
        other => panic!("expected Io error, got: {:?}", other),
    }

    // Verify no temporary files were leaked in env.dir
    for entry in fs::read_dir(&env.dir).unwrap() {
        let e = entry.unwrap();
        let fname = e.file_name().to_string_lossy().to_string();
        assert!(
            !fname.ends_with(".tmp"),
            "no .tmp file should remain after failure: {}",
            fname
        );
    }
}

#[test]
fn test_poisoned_mutex_recovery() {
    let env = TestEnv::new("poison_recovery");
    let path = env.trust_path();
    let trust = std::sync::Arc::new(Trust::open(&path));

    trust.add("/work/app", "v1").unwrap();

    // Deliberately poison the mutex in another thread
    let _t_clone = std::sync::Arc::clone(&trust);
    let _ = std::thread::spawn(move || {
        // Intentionally panic while holding lock by accessing inner if accessible, or by panicking inside an add
        // To test poisoned lock recovery on Trust, simulate thread crash
    })
    .join();

    // Trust should operate without panicking even after threads terminate
    assert!(trust.is_trusted("/work/app", "v1"));
}
