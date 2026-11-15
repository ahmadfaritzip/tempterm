use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use temp_terminal_procs::def::Def;
use temp_terminal_procs::supervisor::{
    Handle, ProcessStatus, Runner, RunnerError, StartOptions, Supervisor, SupervisorError,
};
use temp_terminal_procs::trust::Trust;

#[derive(Clone, Default)]
struct MockHandleState {
    pid: u32,
    history: Vec<u8>,
}

struct MockHandle {
    id: String,
    state: Mutex<MockHandleState>,
}

impl MockHandle {
    fn new(id: String, pid: u32) -> Self {
        Self {
            id,
            state: Mutex::new(MockHandleState {
                pid,
                history: Vec::new(),
            }),
        }
    }
}

impl Handle for MockHandle {
    fn id(&self) -> &str {
        &self.id
    }

    fn pid(&self) -> u32 {
        self.state.lock().unwrap().pid
    }

    fn history(&self) -> Vec<u8> {
        self.state.lock().unwrap().history.clone()
    }

    fn is_alive(&self) -> bool {
        self.state.lock().unwrap().pid != 0
    }
}

struct MockRunner {
    starts: Mutex<Vec<StartOptions>>,
    handles: Mutex<HashMap<String, Arc<MockHandle>>>,
    fail_next: Mutex<Option<String>>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            starts: Mutex::new(Vec::new()),
            handles: Mutex::new(HashMap::new()),
            fail_next: Mutex::new(None),
        }
    }

    fn latest_start(&self) -> Option<StartOptions> {
        self.starts.lock().unwrap().last().cloned()
    }

    fn start_count(&self) -> usize {
        self.starts.lock().unwrap().len()
    }
}

impl Runner for MockRunner {
    fn start(&self, options: StartOptions) -> Result<Arc<dyn Handle>, RunnerError> {
        if let Some(err_msg) = self.fail_next.lock().unwrap().take() {
            return Err(RunnerError::StartFailed(err_msg));
        }
        let handle = Arc::new(MockHandle::new(options.id.clone(), 1234));
        self.starts.lock().unwrap().push(options.clone());
        self.handles
            .lock()
            .unwrap()
            .insert(options.id.clone(), handle.clone());
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
        self.handles.lock().unwrap().remove(id);
    }
}

fn create_test_trust(dir: &str, defs: &[Def]) -> Trust {
    let temp_file = tempfile_path();
    let trust = Trust::open(&temp_file);
    for d in defs {
        trust.add(dir, &d.variant()).expect("add trusted variant");
    }
    trust
}

fn tempfile_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "test_trust_{}_{}_{}.json",
        std::process::id(),
        nanos,
        count
    ))
}

#[test]
fn test_mock_runner_and_handle_contract() {
    let runner = MockRunner::new();
    let opts = StartOptions {
        id: "pane-1".to_string(),
        session: "proc-p1-web".to_string(),
        role: "process".to_string(),
        dir: "/tmp".to_string(),
        command: vec!["cargo".to_string(), "run".to_string()],
        env: vec!["PORT=3000".to_string()],
        exec: true,
        history: Vec::new(),
        cols: 120,
        rows: 36,
    };
    let handle = runner.start(opts).expect("start must succeed");
    assert_eq!(handle.id(), "pane-1");
    assert_eq!(handle.pid(), 1234);
    assert!(runner.get("pane-1").is_some());
    runner.remove("pane-1");
    assert!(runner.get("pane-1").is_none());
}

#[test]
fn test_register_and_retrieve_process() {
    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    let def = Def {
        name: "worker".to_string(),
        command: "cargo run".to_string(),
        dir: "sub".to_string(),
        env: BTreeMap::from([("KEY".to_string(), "VAL".to_string())]),
        ..Default::default()
    };

    supervisor.load("proj-1", "D:/project/app", vec![def.clone()]);

    let procs = supervisor.list("proj-1");
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name(), "worker");
    assert_eq!(procs[0].status, ProcessStatus::Untrusted);
    assert_eq!(procs[0].session, "proc-proj-1-worker");
    assert!(!procs[0].trusted);
}

#[test]
fn test_reject_starting_untrusted_process() {
    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    let def = Def {
        name: "web".to_string(),
        command: "npm start".to_string(),
        ..Default::default()
    };

    supervisor.load("p1", "D:/project/app", vec![def]);

    let res = supervisor.start("p1", "web");
    assert!(matches!(res, Err(SupervisorError::Untrusted(_))));
    assert_eq!(runner.start_count(), 0);

    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Untrusted);
}

#[test]
fn test_trusted_start_enters_starting_and_records_handle() {
    let def = Def {
        name: "web".to_string(),
        command: "npm start".to_string(),
        dir: "frontend".to_string(),
        env: BTreeMap::from([("PORT".to_string(), "8080".to_string())]),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def]);
    let initial_procs = supervisor.list("p1");
    assert_eq!(initial_procs[0].status, ProcessStatus::Stopped);
    assert!(initial_procs[0].trusted);

    supervisor.start("p1", "web").expect("start should succeed");

    assert_eq!(runner.start_count(), 1);
    let start_opts = runner.latest_start().unwrap();
    assert_eq!(start_opts.session, "proc-p1-web");
    assert_eq!(start_opts.role, "process");
    assert_eq!(start_opts.dir, "D:/project/app\\frontend");
    assert!(start_opts.env.contains(&"PORT=8080".to_string()));

    let running_procs = supervisor.list("p1");
    assert_eq!(running_procs[0].status, ProcessStatus::Starting);
    assert_eq!(running_procs[0].pid, 1234);
    assert_eq!(running_procs[0].pane, start_opts.id);
}

#[test]
fn test_start_failure_produces_failed_state_and_error() {
    let def = Def {
        name: "failing".to_string(),
        command: "exit 1".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    *runner.fail_next.lock().unwrap() = Some("spawn error: binary not found".to_string());

    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def]);
    let res = supervisor.start("p1", "failing");

    assert!(matches!(res, Err(SupervisorError::StartFailed(_))));
    let procs = supervisor.list("p1");
    assert_eq!(procs[0].status, ProcessStatus::Failed);
    assert!(procs[0].message.contains("binary not found"));
}

#[test]
fn test_explicit_stop_invokes_runner_remove_and_sets_sticky_stop() {
    let def = Def {
        name: "worker".to_string(),
        command: "run".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "worker").expect("start");

    let pane_id = supervisor.list("p1")[0].pane.clone();
    assert!(runner.get(&pane_id).is_some());

    supervisor.stop("p1", "worker").expect("stop");

    assert!(runner.get(&pane_id).is_none());
    let proc_after_stop = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(proc_after_stop.status, ProcessStatus::Stopped);
    assert_eq!(proc_after_stop.pid, 0);

    // If an exit notification arrives for this stopped process, sticky-stop prevents any restart
    supervisor.handle_exit("p1", "worker", 1, 1);
    let proc_after_exit = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(proc_after_exit.status, ProcessStatus::Stopped);
}

#[test]
fn test_exit_code_zero_produces_exited_state() {
    let def = Def {
        name: "task".to_string(),
        command: "echo done".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "task").expect("start");

    let _p = supervisor.list("p1").into_iter().next().unwrap();
    let gen = supervisor.process_generation("p1", "task").unwrap();

    supervisor.handle_exit("p1", "task", gen, 0);

    let after = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(after.status, ProcessStatus::Exited);
    assert_eq!(after.last_exit, 0);
    assert_eq!(after.pid, 0);
    assert_eq!(after.message, "");
}

#[test]
fn test_nonzero_exit_without_restart_produces_failed_state() {
    let def = Def {
        name: "crasher".to_string(),
        command: "crash".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "crasher").expect("start");

    let gen = supervisor.process_generation("p1", "crasher").unwrap();
    supervisor.handle_exit("p1", "crasher", gen, 42);

    let after = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(after.status, ProcessStatus::Failed);
    assert_eq!(after.last_exit, 42);
    assert_eq!(after.message, "exited (code 42)");
}

#[test]
fn test_late_or_stale_generation_exit_does_not_corrupt_state() {
    let def = Def {
        name: "server".to_string(),
        command: "serve".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "server").expect("start");

    let current_gen = supervisor.process_generation("p1", "server").unwrap();
    assert_eq!(current_gen, 1);

    // Stale exit notification for generation 0 arrives
    supervisor.handle_exit("p1", "server", 0, 137);

    // Current state remains Starting, not Failed or Exited
    let proc = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(proc.status, ProcessStatus::Starting);
    assert_ne!(proc.pid, 0);
}

#[test]
fn test_removing_process_on_load_performs_cleanup() {
    let def1 = Def {
        name: "proc1".to_string(),
        command: "run1".to_string(),
        ..Default::default()
    };
    let def2 = Def {
        name: "proc2".to_string(),
        command: "run2".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def1.clone(), def2.clone()]);
    let supervisor = Supervisor::new(runner.clone(), Some(trust));

    supervisor.load("p1", "D:/project/app", vec![def1.clone(), def2.clone()]);
    assert_eq!(supervisor.list("p1").len(), 2);

    // Load with only def1; def2 was stopped and unhandled so it is deleted
    supervisor.load("p1", "D:/project/app", vec![def1]);
    let list = supervisor.list("p1");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name(), "proc1");
}
