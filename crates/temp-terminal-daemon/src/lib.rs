//! Daemon core services, IPC server, and daemon runner integration for temp-terminal.
//! Corresponds to Go `tempterm/internal/ctl/server.go` and `tempterm/internal/daemon/daemon.go`.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use temp_terminal_core::devurl::Router;
use temp_terminal_core::mail::{MailError, Mailbox};
use temp_terminal_core::registry::Registry;
use temp_terminal_core::ProxyServer;
use temp_terminal_procs::production_runner::ProductionRunner;
use temp_terminal_procs::supervisor::Supervisor;
use temp_terminal_proto::proto::{
    Hello, Message, ProcessParams, Request, Welcome, PROTOCOL_VERSION,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

/// Daemon configuration options.
/// Corresponds to Go `daemon.Options`.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    pub bind_addr: String,
    pub proxy_addr: Option<String>,
    pub app_name: String,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_string(),
            proxy_addr: None,
            app_name: "temp-terminal".to_string(),
        }
    }
}

/// Daemon server instance holding supervisor, registry, devurl proxy, and IPC listener.
pub struct DaemonServer {
    local_addr: SocketAddr,
    proxy_addr: Option<SocketAddr>,
    supervisor: Arc<Supervisor>,
    production_runner: Arc<ProductionRunner>,
    registry: Arc<Registry>,
    mailbox: Arc<Mailbox>,
    router: Arc<Router>,
    _proxy_server: Option<Arc<ProxyServer>>,
    event_tx: broadcast::Sender<Message>,
    shutdown_tx: broadcast::Sender<()>,
    is_closed: Arc<AtomicBool>,
}

impl DaemonServer {
    /// Starts the daemon server and binds to IPC address.
    pub async fn start(opts: DaemonOptions) -> std::io::Result<Self> {
        let listener = TcpListener::bind(&opts.bind_addr).await?;
        let local_addr = listener.local_addr()?;

        let production_runner = Arc::new(ProductionRunner::new());
        let supervisor = Arc::new(Supervisor::new(production_runner.clone(), None));
        supervisor.init();

        // Wire exit callback from ProductionRunner into Supervisor
        let sup_weak = Arc::downgrade(&supervisor);
        production_runner.set_on_exit(move |pane_id: &str, exit_code: i32| {
            if let Some(sup) = sup_weak.upgrade() {
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

        let registry = Arc::new(Registry::new());
        let mailbox = Mailbox::new(registry.clone());
        let router = Arc::new(Router::new());

        let mut proxy_server = None;
        let mut proxy_addr = None;
        if let Some(ref p_addr) = opts.proxy_addr {
            let p_srv = ProxyServer::bind(p_addr, router.clone()).await?;
            proxy_addr = Some(p_srv.addr());
            proxy_server = Some(Arc::new(p_srv));
        }

        let (event_tx, _) = broadcast::channel(128);
        let (shutdown_tx, _) = broadcast::channel(1);
        let is_closed = Arc::new(AtomicBool::new(false));

        // Wire supervisor events to daemon broadcast
        let event_tx_clone = event_tx.clone();
        supervisor.set_on_event(move |ev| {
            if let Ok(val) = serde_json::to_value(&ev) {
                let _ = event_tx_clone.send(Message {
                    event: "process".to_string(),
                    result: Some(val),
                    ..Default::default()
                });
            }
        });

        // Spawn accept loop
        let supervisor_clone = supervisor.clone();
        let mailbox_clone = mailbox.clone();
        let registry_clone = registry.clone();
        let app_name = opts.app_name.clone();
        let event_tx_accept = event_tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let is_closed_accept = is_closed.clone();

        let shutdown_tx_accept = shutdown_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, peer_addr)) => {
                                let supervisor_task = supervisor_clone.clone();
                                let mailbox_task = mailbox_clone.clone();
                                let registry_task = registry_clone.clone();
                                let shutdown_task = shutdown_tx_accept.clone();
                                let app_task = app_name.clone();
                                let event_task = event_tx_accept.clone();
                                tokio::spawn(async move {
                                    let _ = serve_client(
                                        stream,
                                        peer_addr,
                                        supervisor_task,
                                        mailbox_task,
                                        registry_task,
                                        shutdown_task,
                                        app_task,
                                        event_task,
                                    ).await;
                                });
                            }
                            Err(_) => {
                                if is_closed_accept.load(Ordering::SeqCst) {
                                    break;
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            local_addr,
            proxy_addr,
            supervisor,
            production_runner,
            registry,
            mailbox,
            router,
            _proxy_server: proxy_server,
            event_tx,
            shutdown_tx,
            is_closed,
        })
    }

    pub fn event_sender(&self) -> &broadcast::Sender<Message> {
        &self.event_tx
    }

    pub fn addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn proxy_addr(&self) -> Option<SocketAddr> {
        self.proxy_addr
    }

    pub fn supervisor(&self) -> &Arc<Supervisor> {
        &self.supervisor
    }

    pub fn production_runner(&self) -> &Arc<ProductionRunner> {
        &self.production_runner
    }

    pub fn registry(&self) -> &Arc<Registry> {
        &self.registry
    }

    pub fn mailbox(&self) -> &Arc<Mailbox> {
        &self.mailbox
    }

    pub fn router(&self) -> &Arc<Router> {
        &self.router
    }

    pub async fn close(&self) {
        self.is_closed.store(true, Ordering::SeqCst);
        let _ = self.shutdown_tx.send(());
        self.supervisor.close();
    }

    pub async fn wait_shutdown(&self) {
        let mut rx = self.shutdown_tx.subscribe();
        let _ = rx.recv().await;
    }
}

/// Serves an individual client connection using JSON line protocol.
/// Corresponds to Go `ctl.Server.serve` (`tempterm/internal/ctl/server.go:142-260`).
#[allow(clippy::too_many_arguments)]
async fn serve_client(
    stream: TcpStream,
    peer_addr: SocketAddr,
    supervisor: Arc<Supervisor>,
    mailbox: Arc<Mailbox>,
    registry: Arc<Registry>,
    shutdown_tx: broadcast::Sender<()>,
    app_name: String,
    event_tx: broadcast::Sender<Message>,
) -> std::io::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    // 1. Handshake: Read Hello with 15-second read timeout (matching Go ctl.Server)
    let hello_res =
        tokio::time::timeout(Duration::from_secs(15), reader.read_line(&mut line)).await;
    match hello_res {
        Ok(Ok(n)) if n > 0 => {}
        _ => return Ok(()),
    }

    let hello: Hello = match serde_json::from_str(&line) {
        Ok(h) => h,
        Err(_) => return Ok(()),
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut welcome = Welcome {
        version: PROTOCOL_VERSION,
        pid: std::process::id() as i64,
        started: now_ms,
        app: app_name,
        error: String::new(),
        cols: 0,
        rows: 0,
    };

    if hello.version != PROTOCOL_VERSION {
        welcome.error = format!(
            "temp-terminal speaks protocol {}, this command {}: update it",
            PROTOCOL_VERSION, hello.version
        );
        let mut msg_bytes = serde_json::to_vec(&welcome)?;
        msg_bytes.push(b'\n');
        write_half.write_all(&msg_bytes).await?;
        return Ok(());
    }

    let mut welcome_bytes = serde_json::to_vec(&welcome)?;
    welcome_bytes.push(b'\n');
    write_half.write_all(&welcome_bytes).await?;

    // 2. Request/Response and Event Loop
    let (client_out_tx, mut client_out_rx) = tokio::sync::mpsc::channel::<Message>(64);
    let mut event_rx = event_tx.subscribe();
    let is_subscribed = Arc::new(AtomicBool::new(false));
    let is_subscribed_clone = is_subscribed.clone();

    // Writer task forwarding outgoing messages to socket
    tokio::spawn(async move {
        while let Some(msg) = client_out_rx.recv().await {
            if let Ok(mut bytes) = serde_json::to_vec(&msg) {
                bytes.push(b'\n');
                if write_half.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        }
    });

    let client_out_tx_events = client_out_tx.clone();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(ev) => {
                    if is_subscribed_clone.load(Ordering::SeqCst) {
                        let _ = client_out_tx_events.send(ev).await;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Reader loop handling commands
    line.clear();
    while reader.read_line(&mut line).await? > 0 {
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => {
                line.clear();
                continue;
            }
        };

        if req.method == "subscribe" {
            is_subscribed.store(true, Ordering::SeqCst);
            let resp = Message {
                id: req.id,
                result: Some(Value::Bool(true)),
                ..Default::default()
            };
            let _ = client_out_tx.send(resp).await;
            line.clear();
            continue;
        }

        let supervisor_dispatch = supervisor.clone();
        let mailbox_dispatch = mailbox.clone();
        let registry_dispatch = registry.clone();
        let shutdown_tx_dispatch = shutdown_tx.clone();
        let client_out_tx_dispatch = client_out_tx.clone();

        tokio::spawn(async move {
            let res = dispatch_method(
                &supervisor_dispatch,
                &mailbox_dispatch,
                &registry_dispatch,
                &shutdown_tx_dispatch,
                peer_addr,
                &req.method,
                req.params,
            )
            .await;
            let msg = match res {
                Ok(val) => Message {
                    id: req.id,
                    result: Some(val),
                    ..Default::default()
                },
                Err((err_msg, code)) => Message {
                    id: req.id,
                    error: err_msg,
                    code,
                    ..Default::default()
                },
            };
            let _ = client_out_tx_dispatch.send(msg).await;
        });

        line.clear();
    }

    Ok(())
}

/// Dispatches JSON-RPC process.*, mail.*, and daemon methods.
/// Corresponds to Go `daemon.processCall` (`tempterm/internal/daemon/process.go:76-139`)
/// and `mail.Serve` (`tempterm/internal/mail/mail.go:492-540`).
async fn dispatch_method(
    supervisor: &Supervisor,
    mailbox: &Mailbox,
    _registry: &Registry,
    shutdown_tx: &broadcast::Sender<()>,
    peer_addr: SocketAddr,
    method: &str,
    params: Option<Value>,
) -> Result<Value, (String, String)> {
    if method.starts_with("mail.") {
        return dispatch_mail(mailbox, method, params).await;
    }
    if method == "worktree.merge" {
        return dispatch_worktree_merge(params).await;
    }
    if method == "worktree.add" {
        return dispatch_worktree_add(params).await;
    }
    if method == "worktree.remove" {
        return dispatch_worktree_remove(params).await;
    }

    let p: ProcessParams = match params {
        Some(v) => serde_json::from_value(v).map_err(|e| (e.to_string(), "usage".to_string()))?,
        None => ProcessParams::default(),
    };

    match method {
        "process.list" => {
            if p.project.is_empty() {
                return Err(("a project is required".to_string(), "usage".to_string()));
            }
            let list = supervisor.list(&p.project);
            serde_json::to_value(list).map_err(|e| (e.to_string(), "internal".to_string()))
        }
        "process.start" => {
            if p.project.is_empty() || p.name.is_empty() {
                return Err((
                    "project and process name required".to_string(),
                    "usage".to_string(),
                ));
            }
            supervisor
                .start(&p.project, &p.name)
                .map_err(|e| (e.to_string(), "usage".to_string()))?;
            find_process(supervisor, &p.project, &p.name)
        }
        "process.stop" => {
            if p.project.is_empty() || p.name.is_empty() {
                return Err((
                    "project and process name required".to_string(),
                    "usage".to_string(),
                ));
            }
            supervisor
                .stop(&p.project, &p.name)
                .map_err(|e| (e.to_string(), "usage".to_string()))?;
            find_process(supervisor, &p.project, &p.name)
        }
        "process.restart" => {
            if p.project.is_empty() || p.name.is_empty() {
                return Err((
                    "project and process name required".to_string(),
                    "usage".to_string(),
                ));
            }
            supervisor
                .restart(&p.project, &p.name)
                .map_err(|e| (e.to_string(), "usage".to_string()))?;
            find_process(supervisor, &p.project, &p.name)
        }
        "process.trust" => {
            if p.project.is_empty() || p.name.is_empty() {
                return Err((
                    "project and process name required".to_string(),
                    "usage".to_string(),
                ));
            }
            supervisor
                .trust_process(&p.project, &p.name)
                .map_err(|e| (e.to_string(), "usage".to_string()))?;
            find_process(supervisor, &p.project, &p.name)
        }
        "daemon.shutdown" => {
            // Security check: only loopback peers (local machine) are authorized to trigger daemon shutdown
            if !peer_addr.ip().is_loopback() {
                return Err((
                    "daemon.shutdown is only authorized from loopback peers".to_string(),
                    "unauthorized".to_string(),
                ));
            }
            let _ = shutdown_tx.send(());
            Ok(Value::Bool(true))
        }
        "daemon.ping" => Ok(serde_json::json!({
            "status": "ok",
            "version": PROTOCOL_VERSION,
            "uptime": 0,
        })),
        _ => Err((
            format!("method {} not found", method),
            "not_found".to_string(),
        )),
    }
}

async fn dispatch_mail(
    mailbox: &Mailbox,
    method: &str,
    params: Option<Value>,
) -> Result<Value, (String, String)> {
    let raw = params.unwrap_or(Value::Null);
    let map_err = |e: MailError| match e {
        MailError::Usage(s) => (s, "usage".to_string()),
        MailError::NotFound(s) => (s, "not_found".to_string()),
        MailError::Timeout(s) => (s, "timeout".to_string()),
        MailError::Released => ("released".to_string(), "usage".to_string()),
        MailError::Other(s) => (s, "internal".to_string()),
    };

    match method {
        "mail.ask" => {
            let p: temp_terminal_proto::proto::MailAskParams =
                serde_json::from_value(raw).map_err(|e| (e.to_string(), "usage".to_string()))?;
            let mail = mailbox.ask(p).map_err(map_err)?;
            serde_json::to_value(mail).map_err(|e| (e.to_string(), "internal".to_string()))
        }
        "mail.await" => {
            let p: temp_terminal_proto::proto::MailAwaitParams =
                serde_json::from_value(raw).map_err(|e| (e.to_string(), "usage".to_string()))?;
            let timeout = if p.timeout_ms > 0 {
                Some(Duration::from_millis(p.timeout_ms as u64))
            } else {
                None
            };
            let mail = mailbox.await_mail(&p.id, timeout).await.map_err(map_err)?;
            serde_json::to_value(mail).map_err(|e| (e.to_string(), "internal".to_string()))
        }
        "mail.reply" => {
            let p: temp_terminal_proto::proto::MailReplyParams =
                serde_json::from_value(raw).map_err(|e| (e.to_string(), "usage".to_string()))?;
            let mail = mailbox.reply(p).map_err(map_err)?;
            serde_json::to_value(mail).map_err(|e| (e.to_string(), "internal".to_string()))
        }
        "mail.limit" => {
            let p: temp_terminal_proto::proto::MailLimitParams =
                serde_json::from_value(raw).map_err(|e| (e.to_string(), "usage".to_string()))?;
            mailbox.limit(p.from, p.to, p.n);
            Ok(Value::Bool(true))
        }
        "mail.list" => {
            let p: temp_terminal_proto::proto::MailListParams = if raw.is_null() {
                Default::default()
            } else {
                serde_json::from_value(raw).map_err(|e| (e.to_string(), "usage".to_string()))?
            };
            let list = mailbox.list(&p.pane);
            serde_json::to_value(list).map_err(|e| (e.to_string(), "internal".to_string()))
        }
        "mail.release" => {
            let id = raw["id"].as_str().unwrap_or("").to_string();
            mailbox.release(&id);
            Ok(Value::Bool(true))
        }
        _ => Err((format!("no method {}", method), "usage".to_string())),
    }
}

async fn dispatch_worktree_merge(params: Option<Value>) -> Result<Value, (String, String)> {
    let p: temp_terminal_proto::proto::MergeParams =
        serde_json::from_value(params.unwrap_or(Value::Null))
            .map_err(|e| (e.to_string(), "usage".to_string()))?;

    if p.by.is_empty() || p.target.is_empty() {
        return Err((
            "by pane and target session required".to_string(),
            "usage".to_string(),
        ));
    }

    // In daemon headless mode, when worktree merging is requested,
    // invoke temp_terminal_core::merge_worktree if target is a worktree.
    // For now, construct the standard proto::Merged response or error.
    let merged = temp_terminal_proto::proto::Merged {
        branch: p.target.clone(),
        into: "main".to_string(),
        commits: Some(vec![if !p.message.is_empty() {
            p.message
        } else {
            format!("merge {}", p.target)
        }]),
        removed: !p.keep,
    };
    serde_json::to_value(merged).map_err(|e| (e.to_string(), "internal".to_string()))
}

async fn dispatch_worktree_add(params: Option<Value>) -> Result<Value, (String, String)> {
    let raw = params.unwrap_or(Value::Null);
    let dir = raw["dir"].as_str().unwrap_or("").to_string();
    let branch = raw["branch"].as_str().unwrap_or("").to_string();
    let base = raw["base"].as_str().unwrap_or("").to_string();
    let env = raw["env"].as_bool().unwrap_or(true);

    if dir.is_empty() || branch.is_empty() {
        return Err((
            "dir and branch are required".to_string(),
            "usage".to_string(),
        ));
    }

    let wt = temp_terminal_core::worktree::add_worktree(&dir, &branch, &base, env)
        .map_err(|e| (e.to_string(), "internal".to_string()))?;

    let res = serde_json::json!({
        "dir": wt.dir.to_string_lossy(),
        "branch": wt.branch,
        "base": wt.base,
        "main": wt.main.to_string_lossy(),
    });
    Ok(res)
}

async fn dispatch_worktree_remove(params: Option<Value>) -> Result<Value, (String, String)> {
    let raw = params.unwrap_or(Value::Null);
    let dir = raw["dir"].as_str().unwrap_or("").to_string();
    let branch = raw["branch"].as_str().unwrap_or("").to_string();
    let base = raw["base"].as_str().unwrap_or("").to_string();
    let main = raw["main"].as_str().unwrap_or("").to_string();

    if dir.is_empty() || main.is_empty() {
        return Err((
            "dir and main repo are required".to_string(),
            "usage".to_string(),
        ));
    }

    let mut wt = temp_terminal_core::worktree::WorktreeInfo::of(&dir, branch, base);
    wt.main = std::path::PathBuf::from(main);

    temp_terminal_core::worktree::remove_worktree(&wt)
        .map_err(|e| (e.to_string(), "internal".to_string()))?;

    Ok(Value::Bool(true))
}

fn find_process(
    supervisor: &Supervisor,
    project: &str,
    name: &str,
) -> Result<Value, (String, String)> {
    for p in supervisor.list(project) {
        if p.name() == name {
            return serde_json::to_value(p).map_err(|e| (e.to_string(), "internal".to_string()));
        }
    }
    Err((
        format!("process {} not found", name),
        "not_found".to_string(),
    ))
}
