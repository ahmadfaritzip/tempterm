//! Integration tests for CLI subcommands against a live daemon instance.
//! Tests ps, start, stop, restart, trust, ask, reply, inbox and exit codes.

use std::collections::HashMap;

use temp_terminal_core::registry::StartPaneOptions;
use temp_terminal_daemon::{DaemonOptions, DaemonServer};
use temp_terminal_procs::def::Def;

// Include or import CLI logic from `temp-terminal-cli`
#[path = "../src/client.rs"]
mod client;
#[path = "../src/cmd.rs"]
mod cmd;

use client::Client;

#[test]
fn test_cli_flag_parsing_and_defaults() {
    let mut valued = HashMap::new();
    valued.insert("timeout", "timeout");
    valued.insert("t", "timeout");
    valued.insert("file", "file");
    let mut boolean = HashMap::new();
    boolean.insert("wait", "wait");
    boolean.insert("json", "json");

    let args = vec![
        "worker".to_string(),
        "-t=30s".to_string(),
        "--wait".to_string(),
        "--json".to_string(),
        "--".to_string(),
        "extra".to_string(),
    ];

    let f = cmd::parse_flags(&args, &valued, &boolean).expect("parse flags");
    assert_eq!(f.pos, vec!["worker"]);
    assert_eq!(f.vals.get("timeout"), Some(&"30s".to_string()));
    assert!(f.set.get("wait").copied().unwrap_or(false));
    assert!(f.set.get("json").copied().unwrap_or(false));
    assert_eq!(f.rest, vec!["extra"]);

    // Test duration parsing
    assert_eq!(cmd::parse_timeout("30s").unwrap(), 30000);
    assert_eq!(cmd::parse_timeout("500ms").unwrap(), 500);
    assert_eq!(cmd::parse_timeout("2m").unwrap(), 120000);
}

#[tokio::test]
async fn test_cli_subcommands_end_to_end_with_daemon() {
    // 1. Start daemon on ephemeral port
    let daemon = DaemonServer::start(DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: None,
        app_name: "temp-terminal-cli-test".to_string(),
    })
    .await
    .expect("start daemon");

    let daemon_addr = daemon.addr().to_string();

    // 2. Connect CLI client
    let mut client = Client::connect(&daemon_addr).await.expect("connect client");
    assert_eq!(client.welcome().app, "temp-terminal-cli-test");

    // 3. Define a process in supervisor
    #[cfg(windows)]
    let cmd_str = "cmd.exe /c echo cli_test && timeout /t 3 >nul".to_string();
    #[cfg(not(windows))]
    let cmd_str = "sh -c 'echo cli_test; sleep 3'".to_string();

    let def = Def {
        name: "api_proc".to_string(),
        command: cmd_str,
        ..Default::default()
    };
    daemon.supervisor().load("proj_cli", "", vec![def]);

    // 4. Test trust via CLI command
    cmd::run_process_cmd(
        &mut client,
        "trust",
        &[
            "api_proc".to_string(),
            "-p".to_string(),
            "proj_cli".to_string(),
        ],
    )
    .await
    .expect("trust command");

    // 5. Test ps via CLI command
    cmd::run_ps(
        &mut client,
        &[
            "-p".to_string(),
            "proj_cli".to_string(),
            "--json".to_string(),
        ],
    )
    .await
    .expect("ps command");

    // 6. Test start via CLI command
    cmd::run_process_cmd(
        &mut client,
        "start",
        &[
            "api_proc".to_string(),
            "-p".to_string(),
            "proj_cli".to_string(),
        ],
    )
    .await
    .expect("start command");

    // 7. Test stop via CLI command
    cmd::run_process_cmd(
        &mut client,
        "stop",
        &[
            "api_proc".to_string(),
            "-p".to_string(),
            "proj_cli".to_string(),
        ],
    )
    .await
    .expect("stop command");

    // 8. Register recipient pane for mailbox CLI tests
    let _pane = daemon
        .registry()
        .start(StartPaneOptions {
            id: "pane_agent_1".to_string(),
            session: "s1".to_string(),
            project: "proj_cli".to_string(),
            role: "agent".to_string(),
            dir: String::new(),
            command: vec![
                "cmd.exe".to_string(),
                "/c".to_string(),
                "exit 0".to_string(),
            ],
            env: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .unwrap();

    // 9. Test ask with --no-wait
    cmd::run_ask(
        &mut client,
        &[
            "pane_agent_1".to_string(),
            "Please review code".to_string(),
            "--no-wait".to_string(),
        ],
    )
    .await
    .expect("ask command");

    // 10. Test reply
    cmd::run_reply(
        &mut client,
        &["m1".to_string(), "Code looks good".to_string()],
    )
    .await
    .expect("reply command");

    // 11. Test inbox
    cmd::run_inbox(&mut client, &["--json".to_string()])
        .await
        .expect("inbox command");

    // 12. Test daemon ping and shutdown
    cmd::run_daemon_cmd(&mut client, "ping")
        .await
        .expect("daemon ping");
    cmd::run_daemon_cmd(&mut client, "shutdown")
        .await
        .expect("daemon shutdown");

    daemon.close().await;
}
