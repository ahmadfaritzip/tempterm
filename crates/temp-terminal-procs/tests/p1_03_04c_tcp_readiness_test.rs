//! Integration tests for P1-03 Step 4C TCP Readiness detection.
//! Verifies exact Go parity against `tempterm/internal/procs/supervisor.go` and `ports_test.go`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use temp_terminal_procs::def::Def;
use temp_terminal_procs::supervisor::{
    Handle, ProcessStatus, Runner, RunnerError, StartOptions, Supervisor,
};
use temp_terminal_procs::trust::Trust;

fn create_test_trust(dir: &str, defs: &[Def]) -> Trust {
    let temp_dir = std::env::temp_dir();
    let unique = format!(
        "trust-test-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = temp_dir.join(unique);
    let trust = Trust::open(path.to_str().unwrap());
    for d in defs {
        let variant = d.variant();
        trust.add(dir, &variant).expect("add trust");
    }
    trust
}

#[derive(Clone)]
struct MockHandle {
    id: String,
    pid: u32,
    alive: Arc<AtomicBool>,
}

impl Handle for MockHandle {
    fn id(&self) -> &str {
        &self.id
    }

    fn pid(&self) -> u32 {
        self.pid
    }

    fn history(&self) -> Vec<u8> {
        Vec::new()
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

struct MockRunner {
    last_opts: Mutex<Option<StartOptions>>,
    handles: Mutex<std::collections::HashMap<String, Arc<MockHandle>>>,
    removed: Mutex<Vec<String>>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            last_opts: Mutex::new(None),
            handles: Mutex::new(std::collections::HashMap::new()),
            removed: Mutex::new(Vec::new()),
        }
    }
}

impl Runner for MockRunner {
    fn start(&self, options: StartOptions) -> Result<Arc<dyn Handle>, RunnerError> {
        let handle = Arc::new(MockHandle {
            id: options.id.clone(),
            pid: 9999,
            alive: Arc::new(AtomicBool::new(true)),
        });
        *self.last_opts.lock().unwrap() = Some(options.clone());
        self.handles
            .lock()
            .unwrap()
            .insert(options.id.clone(), Arc::clone(&handle));
        Ok(handle)
    }

    fn get(&self, id: &str) -> Option<Arc<dyn Handle>> {
        self.handles
            .lock()
            .unwrap()
            .get(id)
            .map(|h| h.clone() as Arc<dyn Handle>)
    }

    fn remove(&self, id: &str) {
        self.removed.lock().unwrap().push(id.to_string());
    }
}

async fn wait_for_status(
    supervisor: &Supervisor,
    project_id: &str,
    name: &str,
    status: ProcessStatus,
    timeout: Duration,
) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        for p in supervisor.list(project_id) {
            if p.def.name == name && p.status == status {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

#[tokio::test]
async fn test_readiness_immediate_success_ipv4() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let def = Def {
        name: "web".to_string(),
        port: port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(500),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "web").expect("start");

    let ok = wait_for_status(
        &supervisor,
        "p1",
        "web",
        ProcessStatus::Running,
        Duration::from_millis(300),
    )
    .await;
    assert!(
        ok,
        "process should reach Running immediately via IPv4 probe"
    );
}

#[tokio::test]
async fn test_readiness_immediate_success_ipv6() {
    let listener = match tokio::net::TcpListener::bind("[::1]:0").await {
        Ok(l) => l,
        Err(_) => return, // IPv6 not supported on this platform/loopback
    };
    let port = listener.local_addr().unwrap().port();

    let def = Def {
        name: "web6".to_string(),
        port: port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(500),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "web6").expect("start");

    let ok = wait_for_status(
        &supervisor,
        "p1",
        "web6",
        ProcessStatus::Running,
        Duration::from_millis(300),
    )
    .await;
    assert!(ok, "process should reach Running via IPv6 probe");
}

#[tokio::test]
async fn test_readiness_delayed_success() {
    let free_port = temp_terminal_procs::ready::allocate_free_port().unwrap();

    let def = Def {
        name: "delayed".to_string(),
        port: free_port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(800),
            Duration::from_millis(20),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "delayed").expect("start");

    // Initially should be starting
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Starting,
        "should still be Starting before listener binds"
    );

    // Bind listener after delay
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", free_port))
        .await
        .unwrap();

    let ok = wait_for_status(
        &supervisor,
        "p1",
        "delayed",
        ProcessStatus::Running,
        Duration::from_millis(400),
    )
    .await;
    assert!(ok, "process should reach Running after listener opened");
    drop(listener);
}

#[tokio::test]
async fn test_readiness_port_zero_fallback() {
    let def = Def {
        name: "batch".to_string(),
        port: 0,
        url: String::new(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(80),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "batch").expect("start");

    // Before deadline: Starting
    assert_eq!(supervisor.list("p1")[0].status, ProcessStatus::Starting);

    // After alive_delay deadline: Running
    let ok = wait_for_status(
        &supervisor,
        "p1",
        "batch",
        ProcessStatus::Running,
        Duration::from_millis(200),
    )
    .await;
    assert!(ok, "process with port 0 must reach Running on deadline");
}

#[tokio::test]
async fn test_readiness_unresponsive_port_fallback() {
    // Port with nothing listening
    let free_port = temp_terminal_procs::ready::allocate_free_port().unwrap();

    let def = Def {
        name: "unresponsive".to_string(),
        port: free_port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(20),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "unresponsive").expect("start");

    // Initially starting
    assert_eq!(supervisor.list("p1")[0].status, ProcessStatus::Starting);

    // After deadline expires: transitions to Running anyway (Go parity)
    let ok = wait_for_status(
        &supervisor,
        "p1",
        "unresponsive",
        ProcessStatus::Running,
        Duration::from_millis(250),
    )
    .await;
    assert!(
        ok,
        "process with unresponsive port must reach Running when alive_delay expires"
    );
}

#[tokio::test]
async fn test_readiness_process_exit_during_polling_cancels_transition() {
    let free_port = temp_terminal_procs::ready::allocate_free_port().unwrap();

    let def = Def {
        name: "crash".to_string(),
        port: free_port as i64,
        restart: "never".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(200),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "crash").expect("start");

    let gen = supervisor.process_generation("p1", "crash").unwrap();

    // Mark handle as not alive and exit process before alive_delay
    tokio::time::sleep(Duration::from_millis(20)).await;
    supervisor.handle_exit("p1", "crash", gen, 1);

    assert_eq!(supervisor.list("p1")[0].status, ProcessStatus::Failed);

    // Wait past deadline to ensure it never transitions to Running
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Failed,
        "exited process must never transition to Running"
    );
}

#[tokio::test]
async fn test_readiness_stop_during_polling_cancels_readiness() {
    let free_port = temp_terminal_procs::ready::allocate_free_port().unwrap();

    let def = Def {
        name: "stopping".to_string(),
        port: free_port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(200),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "stopping").expect("start");

    tokio::time::sleep(Duration::from_millis(20)).await;
    supervisor.stop("p1", "stopping").expect("stop");
    assert_eq!(supervisor.list("p1")[0].status, ProcessStatus::Stopped);

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Stopped,
        "stopped process must remain Stopped"
    );
}

#[tokio::test]
async fn test_readiness_restart_during_polling_discards_stale_generation() {
    let free_port = temp_terminal_procs::ready::allocate_free_port().unwrap();

    let def = Def {
        name: "restarting".to_string(),
        port: free_port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(200),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "restarting").expect("start");
    let gen1 = supervisor.process_generation("p1", "restarting").unwrap();

    tokio::time::sleep(Duration::from_millis(20)).await;
    supervisor.restart("p1", "restarting").expect("restart");
    let gen2 = supervisor.process_generation("p1", "restarting").unwrap();
    assert!(gen2 > gen1, "restart must increment generation");

    // Manually invoking set_running with old generation must be ignored
    supervisor.set_running("p1", "restarting", gen1, "old-handle");
    assert_eq!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Starting,
        "stale generation set_running must be ignored"
    );
}

#[tokio::test]
async fn test_readiness_close_aborts_polling() {
    let free_port = temp_terminal_procs::ready::allocate_free_port().unwrap();

    let def = Def {
        name: "closing".to_string(),
        port: free_port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(200),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "closing").expect("start");

    supervisor.close();
    assert!(supervisor.is_closed());

    tokio::time::sleep(Duration::from_millis(250)).await;
    // Remains in starting or stopped, but set_running will be refused
    let gen = supervisor.process_generation("p1", "closing").unwrap();
    supervisor.set_running("p1", "closing", gen, "any");
    assert_ne!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Running,
        "closed supervisor must never transition process to Running"
    );
}

#[tokio::test]
async fn test_readiness_success_does_not_clear_restart_history() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let def = Def {
        name: "history".to_string(),
        port: port as i64,
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_limits(Duration::from_secs(60), 10, Duration::from_millis(10))
            .with_readiness_limits(
                Duration::from_millis(200),
                Duration::from_millis(10),
                Duration::from_millis(50),
            ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "history").expect("start");

    let gen1 = supervisor.process_generation("p1", "history").unwrap();
    // Simulate an exit that triggers automatic restart
    supervisor.handle_exit_async("p1", "history", gen1, 1).await;

    // After automatic restart, restarts count is 1
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(supervisor.list("p1")[0].restarts, 1);

    // Wait until readiness succeeds
    let ok = wait_for_status(
        &supervisor,
        "p1",
        "history",
        ProcessStatus::Running,
        Duration::from_millis(300),
    )
    .await;
    assert!(ok, "process should reach Running");

    // Restarts count MUST remain 1 (not cleared by readiness)
    assert_eq!(
        supervisor.list("p1")[0].restarts,
        1,
        "readiness success must not clear restart history"
    );
}

#[tokio::test]
async fn test_run_port_injection_into_environment() {
    let mut env = BTreeMap::new();
    env.insert("FOO".to_string(), "BAR".to_string());
    let def = Def {
        name: "shop".to_string(),
        url: "shop".to_string(),
        env,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def.clone()]);
    supervisor.start("p1", "shop").expect("start");

    let last_opts = runner.last_opts.lock().unwrap().clone().unwrap();
    let port_env = last_opts
        .env
        .iter()
        .find(|e| e.starts_with("PORT="))
        .expect("PORT= must be injected for URL definition without port");

    let port_str = port_env.strip_prefix("PORT=").unwrap();
    let allocated_port: u16 = port_str.parse().expect("valid port number");
    assert!(allocated_port > 0);

    let procs = supervisor.list("p1");
    assert_eq!(procs[0].def.port, allocated_port as i64);
}

#[tokio::test]
async fn test_dead_handle_cannot_transition_to_running_even_if_port_answers() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let def = Def {
        name: "zombie".to_string(),
        port: port as i64,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/test", &[def.clone()]);
    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust)).with_readiness_limits(
            Duration::from_millis(500),
            Duration::from_millis(10),
            Duration::from_millis(50),
        ),
    );
    supervisor.init();

    supervisor.load("p1", "D:/test", vec![def]);
    supervisor.start("p1", "zombie").expect("start");

    // Retrieve handle from runner and simulate immediate process death (is_alive -> false)
    let last_opts = runner.last_opts.lock().unwrap().clone().unwrap();
    let handle = runner
        .handles
        .lock()
        .unwrap()
        .get(&last_opts.id)
        .cloned()
        .unwrap();
    handle.alive.store(false, Ordering::SeqCst);

    let gen = supervisor.process_generation("p1", "zombie").unwrap();

    // Direct invocation of set_running must reject dead handle
    supervisor.set_running("p1", "zombie", gen, &handle.id);
    assert_ne!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Running,
        "set_running must refuse to transition when handle.is_alive() is false"
    );

    // Give background poller a moment: it must abort without setting status to Running
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_ne!(
        supervisor.list("p1")[0].status,
        ProcessStatus::Running,
        "poller must abort and not set Running when handle.is_alive() is false"
    );
}
