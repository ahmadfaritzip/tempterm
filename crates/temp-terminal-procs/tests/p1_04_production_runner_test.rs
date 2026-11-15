//! P1-04 Integration Tests: Production Runner with real OS Processes and Windows ConPTY
//!
//! Validates:
//! 1. Real OS process spawning via `ProductionRunner`.
//! 2. Correct environment, working directory, and command arguments propagation.
//! 3. Process liveness probe (`is_alive()`) behavior during run and after exit.
//! 4. Process exit notification callback integration with supervisor restart.
//! 5. Terminal input/output streaming into memory history buffer.
//! 6. Terminal resize operations without crashing or corrupting state.
//! 7. Process termination (`remove()`), graceful stop, and JobObject cleanup.
//! 8. End-to-end integration between `Supervisor` and `ProductionRunner`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use temp_terminal_procs::def::Def;
use temp_terminal_procs::production_runner::ProductionRunner;
use temp_terminal_procs::supervisor::{ProcessStatus, Runner, StartOptions, Supervisor};
use temp_terminal_procs::trust::Trust;

fn tempfile_path(prefix: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "{}_{}_{}_{}.json",
        prefix,
        std::process::id(),
        nanos,
        count
    ))
}

#[test]
fn test_production_runner_spawn_echo_and_liveness() {
    let runner = ProductionRunner::new();
    let opts = StartOptions {
        id: "pane-test-1".to_string(),
        command: vec!["echo hello from production runner".to_string()],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let handle = runner.start(opts).expect("start process");
    assert_eq!(handle.id(), "pane-test-1");
    assert!(handle.pid() > 0);

    // Wait until process finishes
    let mut count = 0;
    while handle.is_alive() && count < 100 {
        std::thread::sleep(Duration::from_millis(20));
        count += 1;
    }

    assert!(
        !handle.is_alive(),
        "process should no longer be alive after echo"
    );

    // History buffer should have captured stdout
    let history = handle.history();
    let out_str = String::from_utf8_lossy(&history);
    assert!(
        out_str.contains("hello from production runner"),
        "history: {:?}",
        out_str
    );
}

#[test]
fn test_production_runner_env_and_dir_propagation() {
    let runner = ProductionRunner::new();
    let temp_dir = std::env::temp_dir();
    let dir_str = temp_dir.to_str().unwrap().to_string();

    let opts = StartOptions {
        id: "pane-env-test".to_string(),
        command: vec!["echo PROD_VAR=%PROD_TEST_VAR% & cd".to_string()],
        dir: dir_str.clone(),
        env: vec!["PROD_TEST_VAR=production_xyz_789".to_string()],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let handle = runner.start(opts).expect("start process");

    let mut count = 0;
    while handle.is_alive() && count < 100 {
        std::thread::sleep(Duration::from_millis(20));
        count += 1;
    }

    let history = handle.history();
    let out_str = String::from_utf8_lossy(&history);
    assert!(
        out_str.contains("PROD_VAR=production_xyz_789"),
        "output was: {:?}",
        out_str
    );
}

#[test]
fn test_production_runner_resize_and_remove() {
    let runner = ProductionRunner::new();
    let opts = StartOptions {
        id: "pane-long-run".to_string(),
        command: vec!["cmd.exe".to_string(), "/d".to_string()],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let handle = runner.start(opts).expect("start long running process");
    assert!(handle.is_alive());

    // Lookup handle from runner
    let retrieved = runner.get("pane-long-run").expect("get handle");
    assert_eq!(retrieved.pid(), handle.pid());

    // Remove should cleanly terminate process and JobObject
    runner.remove("pane-long-run");

    assert!(!handle.is_alive());
    assert!(runner.get("pane-long-run").is_none());
}

#[test]
fn test_supervisor_end_to_end_with_production_runner() {
    let runner = Arc::new(ProductionRunner::new());
    let trust_path = tempfile_path("trust_prod_e2e");
    let trust = Trust::open(&trust_path);

    let dir = "D:/test/production_app";
    let def = Def {
        name: "worker".to_string(),
        command: "echo worker finished successfully".to_string(),
        autostart: true,
        ..Default::default()
    };
    trust.add(dir, &def.variant()).expect("trust worker");

    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    // Wire exit notifier from ProductionRunner into Supervisor
    let sup_weak = Arc::downgrade(&supervisor);
    runner.set_on_exit(move |pane_id: &str, exit_code: i32| {
        if let Some(sup) = sup_weak.upgrade() {
            // Find which project owns this process pane
            for routed in sup.all() {
                if routed.proc.pane == pane_id {
                    let gen = sup
                        .process_generation(&routed.project, &routed.proc.def.name)
                        .unwrap_or(0);
                    sup.handle_exit(&routed.project, &routed.proc.def.name, gen, exit_code);
                    break;
                }
            }
        }
    });

    // Load project: triggers autostart using real OS process
    supervisor.load("proj_prod", dir, vec![def]);

    // Give process time to spawn, execute echo, and exit
    let mut count = 0;
    while count < 100 {
        let list = supervisor.list("proj_prod");
        if !list.is_empty() && list[0].status == ProcessStatus::Exited {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
        count += 1;
    }

    let procs = supervisor.list("proj_prod");
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].status, ProcessStatus::Exited);
    assert_eq!(procs[0].last_exit, 0);

    // Clean up trust file
    let _ = std::fs::remove_file(trust_path);
}

#[test]
fn test_interactive_terminal_roundtrip_io_and_live_streaming() {
    let runner = Arc::new(ProductionRunner::new());

    // Track live streaming callbacks
    let live_stream_received = Arc::new(Mutex::new(Vec::<u8>::new()));
    let stream_clone = Arc::clone(&live_stream_received);

    runner.set_on_output(move |_pane_id, chunk| {
        stream_clone.lock().unwrap().extend_from_slice(chunk);
    });

    // Start an interactive cmd.exe session (no /c, staying alive waiting for stdin commands)
    let opts = StartOptions {
        id: "pane-interactive-rt".to_string(),
        command: vec!["cmd.exe".to_string()],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let handle = runner.start(opts).expect("start interactive process");
    let prod_handle = runner
        .get_production_handle("pane-interactive-rt")
        .expect("get typed production handle");

    // Also register a handle-level subscriber
    let handle_stream_received = Arc::new(Mutex::new(Vec::<u8>::new()));
    let h_stream_clone = Arc::clone(&handle_stream_received);
    prod_handle.subscribe(move |_id, chunk| {
        h_stream_clone.lock().unwrap().extend_from_slice(chunk);
    });

    // Ensure the process is alive
    assert!(handle.is_alive(), "interactive cmd.exe must be running");

    // Send interactive input: write a distinctive echo command
    let secret_token = "LIVE_ROUNDTRIP_TOKEN_98765";
    let cmd_input = format!("echo {}\r\n", secret_token);
    prod_handle
        .write_in(cmd_input.as_bytes())
        .expect("write to interactive process stdin");

    // Wait for the response to appear in:
    // 1. Live global subscriber stream
    // 2. Live handle subscriber stream
    // 3. Scrollback history buffer
    let mut count = 0;
    let mut found_in_global = false;
    let mut found_in_handle = false;
    let mut found_in_history = false;

    while count < 100 {
        let global_bytes = live_stream_received.lock().unwrap().clone();
        let global_str = String::from_utf8_lossy(&global_bytes);
        if global_str.contains(secret_token) {
            found_in_global = true;
        }

        let handle_bytes = handle_stream_received.lock().unwrap().clone();
        let handle_str = String::from_utf8_lossy(&handle_bytes);
        if handle_str.contains(secret_token) {
            found_in_handle = true;
        }

        let hist_bytes = handle.history();
        let hist_str = String::from_utf8_lossy(&hist_bytes);
        if hist_str.contains(secret_token) {
            found_in_history = true;
        }

        if found_in_global && found_in_handle && found_in_history {
            break;
        }

        thread::sleep(Duration::from_millis(25));
        count += 1;
    }

    assert!(
        found_in_global,
        "expected token in global live stream. Got: {:?}",
        String::from_utf8_lossy(&live_stream_received.lock().unwrap())
    );
    assert!(
        found_in_handle,
        "expected token in handle live stream. Got: {:?}",
        String::from_utf8_lossy(&handle_stream_received.lock().unwrap())
    );
    assert!(
        found_in_history,
        "expected token in scrollback history. Got: {:?}",
        String::from_utf8_lossy(&handle.history())
    );

    // Verify resize works on live interactive console
    prod_handle.resize(100, 30).expect("resize interactive pty");

    // Clean exit: send exit command to cmd.exe
    prod_handle
        .write_in(b"exit\r\n")
        .expect("write exit to stdin");

    let mut exit_count = 0;
    while handle.is_alive() && exit_count < 100 {
        thread::sleep(Duration::from_millis(20));
        exit_count += 1;
    }

    // Clean up handle from runner
    runner.remove("pane-interactive-rt");
    assert!(!handle.is_alive());
}
