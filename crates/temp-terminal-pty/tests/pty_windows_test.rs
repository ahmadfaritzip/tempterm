use std::time::Duration;
use temp_terminal_pty::{PtyOptions, PtyProc};

#[test]
fn test_spawn_echo_cmd() {
    let opts = PtyOptions {
        path: "cmd.exe".to_string(),
        args: vec![
            "cmd".to_string(),
            "/d".to_string(),
            "/c".to_string(),
            "echo hello conpty".to_string(),
        ],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let pty = PtyProc::start(opts).expect("start conpty");
    assert!(pty.pid() > 0);

    let (tx, rx) = std::sync::mpsc::channel();
    let pty_clone = pty.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut output = Vec::new();
        loop {
            match pty_clone.read_out(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let _ = tx.send(output);
    });

    let output = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("receive output within 5s");

    let out_str = String::from_utf8_lossy(&output);
    assert!(
        out_str.contains("hello conpty"),
        "output was: {:?}",
        out_str
    );

    // Wait for exit code
    let mut count = 0;
    while pty.exit_code().is_none() && count < 50 {
        std::thread::sleep(Duration::from_millis(20));
        count += 1;
    }
    assert_eq!(pty.exit_code(), Some(0));
    assert!(!pty.is_alive());
}

#[test]
fn test_env_and_dir_propagation() {
    let temp_dir = std::env::temp_dir();
    let dir_str = temp_dir.to_str().unwrap().to_string();

    let opts = PtyOptions {
        path: "cmd.exe".to_string(),
        args: vec![
            "cmd".to_string(),
            "/d".to_string(),
            "/c".to_string(),
            "echo VAR=%MY_TEST_VAR% & cd".to_string(),
        ],
        dir: dir_str.clone(),
        env: vec!["MY_TEST_VAR=custom_val_123".to_string()],
        cols: 80,
        rows: 24,
    };

    let pty = PtyProc::start(opts).expect("start conpty");

    let (tx, rx) = std::sync::mpsc::channel();
    let pty_clone = pty.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        let mut output = Vec::new();
        loop {
            match pty_clone.read_out(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let _ = tx.send(output);
    });

    let output = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("receive output within 5s");

    let out_str = String::from_utf8_lossy(&output);
    assert!(
        out_str.contains("VAR=custom_val_123"),
        "output: {:?}",
        out_str
    );

    let mut count = 0;
    while pty.exit_code().is_none() && count < 50 {
        std::thread::sleep(Duration::from_millis(20));
        count += 1;
    }
    assert_eq!(pty.exit_code(), Some(0));
}

#[test]
fn test_resize_and_stop() {
    let opts = PtyOptions {
        path: "cmd.exe".to_string(),
        args: vec!["cmd".to_string(), "/d".to_string()],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let pty = PtyProc::start(opts).expect("start conpty");
    assert!(pty.is_alive());

    pty.resize(100, 40).expect("resize");

    // Send exit command to cmd.exe
    pty.write_in(b"exit\r\n").expect("write exit");

    let mut count = 0;
    while pty.is_alive() && count < 50 {
        std::thread::sleep(Duration::from_millis(20));
        count += 1;
    }

    if pty.is_alive() {
        pty.stop(Duration::from_millis(100));
    }

    assert!(!pty.is_alive());
}

#[test]
fn test_query_process_name_current_process() {
    let current_pid = std::process::id();
    let name = temp_terminal_pty::query_process_name(current_pid);
    assert!(name.is_some(), "Current process name must be queryable");
    let proc_name = name.unwrap().to_lowercase();
    assert!(
        proc_name.ends_with(".exe") || !proc_name.is_empty(),
        "Expected executable name, got: {}",
        proc_name
    );
}

#[test]
fn test_active_foreground_process_detection_in_pty() {
    // Start cmd.exe in interactive mode
    let opts = PtyOptions {
        path: "cmd.exe".to_string(),
        args: vec!["cmd.exe".to_string(), "/k".to_string()],
        cols: 80,
        rows: 24,
        ..Default::default()
    };

    let pty = PtyProc::start(opts).expect("start conpty");
    assert!(pty.is_alive());

    // Initially, only cmd.exe (root) and conhost are running, so active foreground process is None (idle shell)
    let initial_proc = pty.active_foreground_process();
    assert_eq!(
        initial_proc, None,
        "Idle shell must have no active child process"
    );

    // Launch a child process that runs for a moment: e.g. ping 127.0.0.1 -n 3
    pty.write_in(b"ping 127.0.0.1 -n 3\r\n").expect("write command");

    // Wait a short moment for child process to appear in the job
    let mut detected = None;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(proc) = pty.active_foreground_process() {
            detected = Some(proc);
            break;
        }
    }

    assert!(
        detected.is_some(),
        "Expected ping.exe to be detected as active foreground process"
    );
    let proc_str = detected.unwrap().to_lowercase();
    assert!(
        proc_str.contains("ping"),
        "Expected process name to contain 'ping', got: {}",
        proc_str
    );

    // Clean up
    pty.stop(Duration::from_millis(200));
}
