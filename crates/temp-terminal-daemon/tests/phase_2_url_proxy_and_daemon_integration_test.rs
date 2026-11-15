//! Phase 2 Integration Tests: URL Proxy, Registry, and Daemon IPC Server.
//! Corresponds to Go `tempterm/internal/devurl/server_test.go`, `tempterm/internal/session/registry_test.go`,
//! and `tempterm/internal/ctl/server_test.go`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use temp_terminal_core::devurl::{Route, Router};
use temp_terminal_core::registry::{Registry, StartPaneOptions};
use temp_terminal_core::ProxyServer;
use temp_terminal_daemon::{DaemonOptions, DaemonServer};
use temp_terminal_procs::def::Def;
use temp_terminal_procs::supervisor::ProcessStatus;
use temp_terminal_proto::proto::{
    Hello, Message, ProcessParams, Request, Welcome, PROTOCOL_VERSION,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

fn temp_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}_{}_{}_{}", prefix, std::process::id(), nanos, count)
}

async fn wait_for_condition<F>(timeout: Duration, condition: F) -> bool
where
    F: Fn() -> bool,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[tokio::test]
async fn test_url_proxy_forwards_to_backend_and_injects_headers() {
    // 1. Start mock HTTP backend server on loopback
    let backend_ln = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind backend");
    let backend_port = backend_ln.local_addr().unwrap().port();

    let backend_received_req = Arc::new(tokio::sync::Mutex::new(String::new()));
    let backend_rec_clone = backend_received_req.clone();

    tokio::spawn(async move {
        if let Ok((mut stream, _)) = backend_ln.accept().await {
            let mut buf = vec![0u8; 4096];
            if let Ok(n) = stream.read(&mut buf).await {
                let s = String::from_utf8_lossy(&buf[..n]).to_string();
                *backend_rec_clone.lock().await = s;
                let body = "shop says hi at /hello?x=1";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        }
    });

    // 2. Start DevURL ProxyServer
    let router = Arc::new(Router::new());
    router.set(vec![Route {
        host: "shop.localhost".to_string(),
        project: "x".to_string(),
        process: "web".to_string(),
        port: backend_port,
        running: true,
        status: String::new(),
    }]);

    let proxy = ProxyServer::bind("127.0.0.1:0", router)
        .await
        .expect("bind proxy");
    let proxy_port = proxy.port();

    // 3. Connect as client asking for shop.localhost
    let mut client = TcpStream::connect(("127.0.0.1", proxy_port))
        .await
        .expect("connect proxy");

    let req = format!(
        "GET /hello?x=1 HTTP/1.1\r\nHost: shop.localhost:{}\r\nUser-Agent: test\r\n\r\n",
        proxy_port
    );
    client.write_all(req.as_bytes()).await.expect("write req");

    let mut resp_buf = vec![0u8; 4096];
    let n = client.read(&mut resp_buf).await.expect("read response");
    let resp_str = String::from_utf8_lossy(&resp_buf[..n]);

    assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
    assert!(resp_str.contains("shop says hi at /hello?x=1"));

    // 4. Verify forwarded headers received by backend
    let received = backend_received_req.lock().await.clone();
    assert!(received.contains("X-Forwarded-Proto: https"));
    assert!(received.contains(&format!("X-Forwarded-Host: shop.localhost:{}", proxy_port)));
    assert!(received.contains("X-Forwarded-For: 127.0.0.1"));

    proxy.close().await;
}

#[tokio::test]
async fn test_url_proxy_handles_unknown_host_and_stopped_process() {
    let router = Arc::new(Router::new());
    router.set(vec![Route {
        host: "shop.localhost".to_string(),
        project: "myproj".to_string(),
        process: "web".to_string(),
        port: 8080,
        running: false, // Stopped
        status: "stopped".to_string(),
    }]);

    let proxy = ProxyServer::bind("127.0.0.1:0", router)
        .await
        .expect("bind proxy");
    let proxy_port = proxy.port();

    // Test stopped process -> 502 Bad Gateway
    {
        let mut client = TcpStream::connect(("127.0.0.1", proxy_port))
            .await
            .expect("connect proxy");
        let req = format!(
            "GET / HTTP/1.1\r\nHost: shop.localhost:{}\r\n\r\n",
            proxy_port
        );
        client.write_all(req.as_bytes()).await.unwrap();

        let mut buf = vec![0u8; 4096];
        let n = client.read(&mut buf).await.unwrap();
        let resp = String::from_utf8_lossy(&buf[..n]);

        assert!(resp.starts_with("HTTP/1.1 502 Bad Gateway"));
        assert!(resp.contains("web isn&#39;t running") || resp.contains("web isn't running"));
        assert!(resp.contains("myproj"));
    }

    // Test unknown host -> 404 Not Found
    {
        let mut client = TcpStream::connect(("127.0.0.1", proxy_port))
            .await
            .expect("connect proxy");
        let req = format!(
            "GET / HTTP/1.1\r\nHost: unknown.localhost:{}\r\n\r\n",
            proxy_port
        );
        client.write_all(req.as_bytes()).await.unwrap();

        let mut buf = vec![0u8; 4096];
        let n = client.read(&mut buf).await.unwrap();
        let resp = String::from_utf8_lossy(&buf[..n]);

        assert!(resp.starts_with("HTTP/1.1 404 Not Found"));
        assert!(resp.contains("No process at <code>unknown.localhost</code>"));
        assert!(resp.contains("shop.localhost"));
    }

    proxy.close().await;
}

#[test]
fn test_core_registry_lifecycle_and_pty_management() {
    let registry = Registry::new();
    let pane_id = temp_id("pane_reg");

    #[cfg(windows)]
    let cmd = vec![
        "cmd.exe".to_string(),
        "/c".to_string(),
        "echo reg_hello".to_string(),
    ];
    #[cfg(not(windows))]
    let cmd = vec!["echo".to_string(), "reg_hello".to_string()];

    let pane = registry
        .start(StartPaneOptions {
            id: pane_id.clone(),
            session: "sess_1".to_string(),
            project: "proj_1".to_string(),
            role: "agent".to_string(),
            dir: String::new(),
            command: cmd,
            env: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .expect("start pane in registry");

    assert_eq!(pane.id(), pane_id);
    assert_eq!(registry.list().len(), 1);

    // Verify duplicate ID returns Exists
    assert!(registry
        .start(StartPaneOptions {
            id: pane_id.clone(),
            ..Default::default()
        })
        .is_err());

    // Give time for echo process to finish
    std::thread::sleep(Duration::from_millis(150));

    // Remove pane
    registry.remove(&pane_id);
    assert_eq!(registry.list().len(), 0);
    assert!(registry.get(&pane_id).is_none());
}

#[tokio::test]
async fn test_daemon_server_ipc_roundtrip_and_production_runner_integration() {
    let daemon = DaemonServer::start(DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: Some("127.0.0.1:0".to_string()),
        app_name: "temp-terminal-test".to_string(),
    })
    .await
    .expect("start daemon server");

    let daemon_addr = daemon.addr();

    // 1. Connect IPC client
    let stream = TcpStream::connect(daemon_addr)
        .await
        .expect("connect daemon");
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // 2. Perform Handshake (Send Hello, read Welcome)
    let hello = Hello {
        kind: "cli".to_string(),
        version: PROTOCOL_VERSION,
        client: "test_runner".to_string(),
        pane: String::new(),
    };
    let mut hello_bytes = serde_json::to_vec(&hello).unwrap();
    hello_bytes.push(b'\n');
    write_half.write_all(&hello_bytes).await.unwrap();

    let mut welcome_line = String::new();
    reader.read_line(&mut welcome_line).await.unwrap();
    let welcome: Welcome = serde_json::from_str(&welcome_line).expect("parse welcome");

    assert_eq!(welcome.version, PROTOCOL_VERSION);
    assert_eq!(welcome.app, "temp-terminal-test");
    assert!(welcome.error.is_empty());

    // 3. Configure a definition in the supervisor through production runner
    #[cfg(windows)]
    let cmd = "cmd.exe /c echo daemon_prod_hello && timeout /t 5 >nul".to_string();
    #[cfg(not(windows))]
    let cmd = "sh -c 'echo daemon_prod_hello; sleep 5'".to_string();

    let def = Def {
        name: "worker".to_string(),
        command: cmd.clone(),
        ..Default::default()
    };
    daemon.supervisor().load("proj_ipc", "", vec![def]);
    daemon
        .supervisor()
        .trust_process("proj_ipc", "worker")
        .unwrap();

    // 4. Send process.start Request over IPC socket
    let params = ProcessParams {
        project: "proj_ipc".to_string(),
        name: "worker".to_string(),
        via: String::new(),
    };
    let req = Request {
        id: 101,
        method: "process.start".to_string(),
        params: Some(serde_json::to_value(&params).unwrap()),
    };
    let mut req_bytes = serde_json::to_vec(&req).unwrap();
    req_bytes.push(b'\n');
    write_half.write_all(&req_bytes).await.unwrap();

    // Read Response Message
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).await.unwrap();
    let msg: Message = serde_json::from_str(&resp_line).expect("parse response message");

    assert_eq!(msg.id, 101);
    assert!(msg.error.is_empty(), "unexpected error: {}", msg.error);
    assert!(msg.result.is_some());

    // Verify process transitioned to Running (or Exited if finished) via Supervisor + ProductionRunner
    assert!(
        wait_for_condition(Duration::from_secs(5), || {
            let list = daemon.supervisor().list("proj_ipc");
            !list.is_empty()
                && (list[0].status == ProcessStatus::Running
                    || list[0].status == ProcessStatus::Starting)
        })
        .await,
        "process started via daemon IPC must reach Starting or Running status"
    );

    // 5. Query process.list over IPC socket
    let list_req = Request {
        id: 102,
        method: "process.list".to_string(),
        params: Some(
            serde_json::to_value(&ProcessParams {
                project: "proj_ipc".to_string(),
                name: String::new(),
                via: String::new(),
            })
            .unwrap(),
        ),
    };
    let mut list_bytes = serde_json::to_vec(&list_req).unwrap();
    list_bytes.push(b'\n');
    write_half.write_all(&list_bytes).await.unwrap();

    let mut list_resp_line = String::new();
    reader.read_line(&mut list_resp_line).await.unwrap();
    let list_msg: Message = serde_json::from_str(&list_resp_line).unwrap();

    assert_eq!(list_msg.id, 102);
    let list_val = list_msg.result.expect("list result present");
    let procs_array = list_val.as_array().expect("result is array");
    assert_eq!(procs_array.len(), 1);
    assert_eq!(procs_array[0]["name"], "worker");

    // 6. Stop process over IPC
    let stop_req = Request {
        id: 103,
        method: "process.stop".to_string(),
        params: Some(serde_json::to_value(&params).unwrap()),
    };
    let mut stop_bytes = serde_json::to_vec(&stop_req).unwrap();
    stop_bytes.push(b'\n');
    write_half.write_all(&stop_bytes).await.unwrap();

    let mut stop_resp_line = String::new();
    reader.read_line(&mut stop_resp_line).await.unwrap();
    let stop_msg: Message = serde_json::from_str(&stop_resp_line).unwrap();
    assert_eq!(stop_msg.id, 103);
    assert!(stop_msg.error.is_empty());

    assert!(
        wait_for_condition(Duration::from_secs(1), || {
            let list = daemon.supervisor().list("proj_ipc");
            !list.is_empty() && list[0].status == ProcessStatus::Stopped
        })
        .await,
        "process stopped via daemon IPC must reach Stopped status"
    );

    // 7. Test process.restart over IPC socket
    let restart_req = Request {
        id: 104,
        method: "process.restart".to_string(),
        params: Some(serde_json::to_value(&params).unwrap()),
    };
    let mut restart_bytes = serde_json::to_vec(&restart_req).unwrap();
    restart_bytes.push(b'\n');
    write_half.write_all(&restart_bytes).await.unwrap();

    let mut restart_resp_line = String::new();
    reader.read_line(&mut restart_resp_line).await.unwrap();
    let restart_msg: Message = serde_json::from_str(&restart_resp_line).unwrap();
    assert_eq!(restart_msg.id, 104);
    assert!(
        restart_msg.error.is_empty(),
        "restart error: {}",
        restart_msg.error
    );

    // 8. Test process.trust over IPC socket
    let def2 = Def {
        name: "worker2".to_string(),
        command: cmd,
        ..Default::default()
    };
    daemon.supervisor().load("proj_ipc", "", vec![def2]);

    let trust_req = Request {
        id: 105,
        method: "process.trust".to_string(),
        params: Some(
            serde_json::to_value(&ProcessParams {
                project: "proj_ipc".to_string(),
                name: "worker2".to_string(),
                via: String::new(),
            })
            .unwrap(),
        ),
    };
    let mut trust_bytes = serde_json::to_vec(&trust_req).unwrap();
    trust_bytes.push(b'\n');
    write_half.write_all(&trust_bytes).await.unwrap();

    let mut trust_resp_line = String::new();
    reader.read_line(&mut trust_resp_line).await.unwrap();
    let trust_msg: Message = serde_json::from_str(&trust_resp_line).unwrap();
    assert_eq!(trust_msg.id, 105);
    assert!(
        trust_msg.error.is_empty(),
        "trust error: {}",
        trust_msg.error
    );

    // 9. Test mail.* over IPC socket
    // Register mock pane in registry so mail recipient exists
    let _pane = daemon
        .registry()
        .start(StartPaneOptions {
            id: "pane_agent".to_string(),
            session: "sess_a".to_string(),
            project: "proj_ipc".to_string(),
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

    let ask_req = Request {
        id: 106,
        method: "mail.ask".to_string(),
        params: Some(
            serde_json::to_value(&temp_terminal_proto::proto::MailAskParams {
                from: "pane_lead".to_string(),
                to: "pane_agent".to_string(),
                text: "Hello from IPC".to_string(),
                no_wait: true,
                ..Default::default()
            })
            .unwrap(),
        ),
    };
    let mut ask_bytes = serde_json::to_vec(&ask_req).unwrap();
    ask_bytes.push(b'\n');
    write_half.write_all(&ask_bytes).await.unwrap();

    let mut ask_resp_line = String::new();
    reader.read_line(&mut ask_resp_line).await.unwrap();
    let ask_msg: Message = serde_json::from_str(&ask_resp_line).unwrap();
    assert_eq!(ask_msg.id, 106);
    assert!(
        ask_msg.error.is_empty(),
        "mail.ask error: {}",
        ask_msg.error
    );

    // Query mail.list
    let list_mail_req = Request {
        id: 107,
        method: "mail.list".to_string(),
        params: Some(
            serde_json::to_value(&temp_terminal_proto::proto::MailListParams {
                pane: "pane_agent".to_string(),
            })
            .unwrap(),
        ),
    };
    let mut list_mail_bytes = serde_json::to_vec(&list_mail_req).unwrap();
    list_mail_bytes.push(b'\n');
    write_half.write_all(&list_mail_bytes).await.unwrap();

    let mut list_mail_resp_line = String::new();
    reader.read_line(&mut list_mail_resp_line).await.unwrap();
    let list_mail_msg: Message = serde_json::from_str(&list_mail_resp_line).unwrap();
    assert_eq!(list_mail_msg.id, 107);
    let mails: Vec<temp_terminal_proto::proto::Mail> =
        serde_json::from_value(list_mail_msg.result.unwrap()).unwrap();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].text, "Hello from IPC");

    // 10. Complete Phase 3 IPC coverage: mail.reply, mail.await, mail.limit, mail.release
    // Test mail.limit
    let limit_req = Request {
        id: 108,
        method: "mail.limit".to_string(),
        params: Some(
            serde_json::to_value(&temp_terminal_proto::proto::MailLimitParams {
                from: "pane_lead".to_string(),
                to: "pane_agent".to_string(),
                n: 10,
            })
            .unwrap(),
        ),
    };
    let mut limit_bytes = serde_json::to_vec(&limit_req).unwrap();
    limit_bytes.push(b'\n');
    write_half.write_all(&limit_bytes).await.unwrap();

    let mut limit_resp_line = String::new();
    reader.read_line(&mut limit_resp_line).await.unwrap();
    let limit_msg: Message = serde_json::from_str(&limit_resp_line).unwrap();
    assert_eq!(limit_msg.id, 108);
    assert!(
        limit_msg.error.is_empty(),
        "mail.limit error: {}",
        limit_msg.error
    );

    // Test mail.reply
    let reply_req = Request {
        id: 109,
        method: "mail.reply".to_string(),
        params: Some(
            serde_json::to_value(&temp_terminal_proto::proto::MailReplyParams {
                id: mails[0].id.clone(),
                text: "Replied over IPC".to_string(),
                from: "pane_agent".to_string(),
            })
            .unwrap(),
        ),
    };
    let mut reply_bytes = serde_json::to_vec(&reply_req).unwrap();
    reply_bytes.push(b'\n');
    write_half.write_all(&reply_bytes).await.unwrap();

    let mut reply_resp_line = String::new();
    reader.read_line(&mut reply_resp_line).await.unwrap();
    let reply_msg: Message = serde_json::from_str(&reply_resp_line).unwrap();
    assert_eq!(reply_msg.id, 109);
    assert!(
        reply_msg.error.is_empty(),
        "mail.reply error: {}",
        reply_msg.error
    );

    // Test mail.await (already replied, returns immediately)
    let await_req = Request {
        id: 110,
        method: "mail.await".to_string(),
        params: Some(
            serde_json::to_value(&temp_terminal_proto::proto::MailAwaitParams {
                id: mails[0].id.clone(),
                timeout_ms: 1000,
            })
            .unwrap(),
        ),
    };
    let mut await_bytes = serde_json::to_vec(&await_req).unwrap();
    await_bytes.push(b'\n');
    write_half.write_all(&await_bytes).await.unwrap();

    let mut await_resp_line = String::new();
    reader.read_line(&mut await_resp_line).await.unwrap();
    let await_msg: Message = serde_json::from_str(&await_resp_line).unwrap();
    assert_eq!(await_msg.id, 110);
    assert!(
        await_msg.error.is_empty(),
        "mail.await error: {}",
        await_msg.error
    );

    // Test mail.release
    let release_req = Request {
        id: 111,
        method: "mail.release".to_string(),
        params: Some(serde_json::json!({ "id": mails[0].id.clone() })),
    };
    let mut release_bytes = serde_json::to_vec(&release_req).unwrap();
    release_bytes.push(b'\n');
    write_half.write_all(&release_bytes).await.unwrap();

    let mut release_resp_line = String::new();
    reader.read_line(&mut release_resp_line).await.unwrap();
    let release_msg: Message = serde_json::from_str(&release_resp_line).unwrap();
    assert_eq!(release_msg.id, 111);
    assert!(
        release_msg.error.is_empty(),
        "mail.release error: {}",
        release_msg.error
    );

    daemon.close().await;
}
