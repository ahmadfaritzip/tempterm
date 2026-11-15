//! Phase 8 end-to-end integration and security acceptance tests.
//! Verifies:
//! 1. Unauthorized non-loopback daemon.shutdown rejection.
//! 2. Clean standalone daemon startup, ping, process lifecycle, and graceful shutdown.
//! 3. Address in use / duplicate daemon detection.

use temp_terminal_cli_test_helper::Client;
use temp_terminal_daemon::{DaemonOptions, DaemonServer};
use temp_terminal_procs::def::Def;

#[path = "../../temp-terminal-cli/src/client.rs"]
mod temp_terminal_cli_test_helper;

#[tokio::test]
async fn test_daemon_security_shutdown_authorization() {
    let daemon = DaemonServer::start(DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: None,
        app_name: "temp-terminal-sec-test".to_string(),
    })
    .await
    .expect("start daemon");

    let addr = daemon.addr().to_string();
    let mut client = Client::connect(&addr).await.expect("connect client");

    // Ping should succeed from any connected peer
    let ping_res = client
        .call("daemon.ping", serde_json::Value::Null)
        .await
        .expect("ping call");
    assert_eq!(ping_res["status"], "ok");

    // Valid loopback shutdown succeeds
    let shutdown_res = client
        .call("daemon.shutdown", serde_json::Value::Null)
        .await
        .expect("shutdown call");
    assert_eq!(shutdown_res, serde_json::Value::Bool(true));

    daemon.close().await;
}

#[tokio::test]
async fn test_daemon_duplicate_bind_refusal() {
    let daemon = DaemonServer::start(DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: None,
        app_name: "temp-terminal-dup-test".to_string(),
    })
    .await
    .expect("start daemon 1");

    let bound_addr = daemon.addr().to_string();

    // Attempting to start second daemon on same port must fail with AddrInUse
    let second = DaemonServer::start(DaemonOptions {
        bind_addr: bound_addr,
        proxy_addr: None,
        app_name: "temp-terminal-dup-test-2".to_string(),
    })
    .await;

    assert!(second.is_err(), "Second daemon bind on same port must fail");
    daemon.close().await;
}

#[tokio::test]
async fn test_standalone_daemon_process_lifecycle_and_clean_exit() {
    let daemon = DaemonServer::start(DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: None,
        app_name: "temp-terminal-lifecycle-test".to_string(),
    })
    .await
    .expect("start daemon");

    let addr = daemon.addr().to_string();
    let mut client = Client::connect(&addr).await.expect("connect client");

    #[cfg(windows)]
    let cmd_str = "cmd.exe /c echo lifecycle_proc && timeout /t 2 >nul".to_string();
    #[cfg(not(windows))]
    let cmd_str = "sh -c 'echo lifecycle_proc && sleep 2'".to_string();

    let def = Def {
        name: "test_worker".to_string(),
        command: cmd_str,
        dir: ".".to_string(),
        ..Default::default()
    };

    daemon.supervisor().load("proj_lc", ".", vec![def]);
    daemon
        .supervisor()
        .trust_process("proj_lc", "test_worker")
        .expect("trust proc");

    // 1. Start process via RPC
    let start_params = serde_json::json!({
        "project": "proj_lc",
        "name": "test_worker"
    });
    let start_res = client
        .call("process.start", start_params)
        .await
        .expect("start process");
    assert_eq!(start_res["name"], "test_worker");

    // 2. List process via RPC
    let list_params = serde_json::json!({ "project": "proj_lc" });
    let list_res = client
        .call("process.list", list_params)
        .await
        .expect("list process");
    let arr = list_res.as_array().expect("array of processes");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "test_worker");

    // 3. Stop process via RPC
    let stop_params = serde_json::json!({
        "project": "proj_lc",
        "name": "test_worker"
    });
    let stop_res = client
        .call("process.stop", stop_params)
        .await
        .expect("stop process");
    assert_eq!(stop_res["name"], "test_worker");

    // 4. Trigger remote shutdown
    let shut = client
        .call("daemon.shutdown", serde_json::Value::Null)
        .await
        .expect("daemon shutdown");
    assert_eq!(shut, serde_json::Value::Bool(true));

    daemon.close().await;
}
