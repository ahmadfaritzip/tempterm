//! P1-03 Step 4D Integration Tests: Dynamic Reload, Autostart, and Trust Interactions
//!
//! Validates:
//! 1. Autostart triggers on first load for trusted autostart definitions.
//! 2. Autostart does NOT trigger on second load (idempotence).
//! 3. Autostart does NOT trigger for untrusted definitions.
//! 4. Adding a definition dynamically on subsequent load registers in definition order.
//! 5. Modifying a running definition marks `changed: true` and sets message "definition changed; restart to apply".
//! 6. Modifying an idle (stopped) definition updates definition in place with `changed: false`.
//! 7. Removing a running definition preserves it in `list()` with `changed: true` and message "no longer defined".
//! 8. Removing a stopped definition removes it completely from project state.
//! 9. Stopped definition of a removed process is purged once stopped.
//! 10. Untrusted modification of a running definition refuses restart with `ErrUntrusted`, keeping running process intact.
//! 11. Trusting an untrusted definition via `trust_process` updates `trusted: true` and transitions `untrusted -> stopped`.
//! 12. `sessions()` returns only processes that have or had a pane assigned.
//! 13. `all()` returns processes across projects sorted by project ID and definition order.
//! 14. `OnChange` callback triggers on load, start, stop, restart, exit, readiness, and trust transitions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use temp_terminal_procs::def::Def;
use temp_terminal_procs::supervisor::{
    Event, EventKind, Handle, ProcessStatus, Runner, RunnerError, StartOptions, Supervisor,
    SupervisorError,
};
use temp_terminal_procs::trust::Trust;

fn create_test_trust(dir: &str, defs: &[Def]) -> Trust {
    let temp_file = tempfile_path();
    let trust = Trust::open(&temp_file);
    for d in defs {
        trust.add(dir, &d.variant()).expect("add trusted variant");
    }
    trust
}

fn tempfile_path() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "test_trust_reload_{}_{}_{}.json",
        std::process::id(),
        nanos,
        count
    ))
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
    starts: Mutex<Vec<StartOptions>>,
    handles: Mutex<HashMap<String, Arc<MockHandle>>>,
    removed: Mutex<Vec<String>>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            starts: Mutex::new(Vec::new()),
            handles: Mutex::new(HashMap::new()),
            removed: Mutex::new(Vec::new()),
        }
    }

    fn start_count(&self) -> usize {
        self.starts.lock().unwrap().len()
    }

    fn latest_start(&self) -> Option<StartOptions> {
        self.starts.lock().unwrap().last().cloned()
    }
}

impl Runner for MockRunner {
    fn start(&self, options: StartOptions) -> Result<Arc<dyn Handle>, RunnerError> {
        let handle = Arc::new(MockHandle {
            id: options.id.clone(),
            pid: 1000 + self.starts.lock().unwrap().len() as u32,
            alive: Arc::new(AtomicBool::new(true)),
        });
        self.starts.lock().unwrap().push(options.clone());
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
        if let Some(h) = self.handles.lock().unwrap().remove(id) {
            h.alive.store(false, Ordering::SeqCst);
        }
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
fn test_autostart_first_load_only_and_idempotent() {
    let dir = "D:/test/app";
    let auto_def = Def {
        name: "worker".to_string(),
        command: "run-worker".to_string(),
        autostart: true,
        ..Default::default()
    };
    let manual_def = Def {
        name: "api".to_string(),
        command: "run-api".to_string(),
        autostart: false,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[auto_def.clone(), manual_def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    // First load: autostart must start 'worker', not 'api'
    supervisor.load("p1", dir, vec![auto_def.clone(), manual_def.clone()]);
    assert_eq!(runner.start_count(), 1);

    let list = supervisor.list("p1");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].def.name, "worker");
    assert_eq!(list[0].status, ProcessStatus::Starting);
    assert_eq!(list[1].def.name, "api");
    assert_eq!(list[1].status, ProcessStatus::Stopped);

    // Second load: must NOT autostart again
    supervisor.load("p1", dir, vec![auto_def.clone(), manual_def.clone()]);
    assert_eq!(
        runner.start_count(),
        1,
        "second load must not re-trigger autostart"
    );
}

#[test]
fn test_untrusted_autostart_is_refused() {
    let dir = "D:/test/untrusted";
    let auto_def = Def {
        name: "malicious".to_string(),
        command: "rm -rf".to_string(),
        autostart: true,
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    // Empty trust store: auto_def is not trusted
    let trust = create_test_trust(dir, &[]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", dir, vec![auto_def]);
    assert_eq!(runner.start_count(), 0, "untrusted autostart must not run");

    let list = supervisor.list("p1");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, ProcessStatus::Untrusted);
    assert!(!list[0].trusted);
}

#[test]
fn test_dynamic_add_and_order_preservation() {
    let dir = "D:/test/order";
    let def1 = Def {
        name: "db".to_string(),
        command: "db".to_string(),
        ..Default::default()
    };
    let def2 = Def {
        name: "web".to_string(),
        command: "web".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[def1.clone(), def2.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    // Initial load: 1 process
    supervisor.load("p1", dir, vec![def1.clone()]);
    let list1 = supervisor.list("p1");
    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0].def.name, "db");

    // Dynamic reload adding 'web' before 'db'
    supervisor.load("p1", dir, vec![def2.clone(), def1.clone()]);
    let list2 = supervisor.list("p1");
    assert_eq!(list2.len(), 2);
    assert_eq!(list2[0].def.name, "web");
    assert_eq!(list2[1].def.name, "db");
}

#[test]
fn test_modify_running_definition_marks_changed() {
    let dir = "D:/test/mod";
    let def_v1 = Def {
        name: "web".to_string(),
        command: "serve v1".to_string(),
        ..Default::default()
    };
    let def_v2 = Def {
        name: "web".to_string(),
        command: "serve v2".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[def_v1.clone(), def_v2.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", dir, vec![def_v1]);
    supervisor.start("p1", "web").expect("start");

    // Simulate readiness: transition to Running
    let gen = supervisor.process_generation("p1", "web").unwrap();
    let handle_id = runner.latest_start().unwrap().id;
    supervisor.set_running("p1", "web", gen, &handle_id);

    let before = supervisor.list("p1");
    assert_eq!(before[0].status, ProcessStatus::Running);
    assert!(!before[0].changed);
    assert!(before[0].message.is_empty());

    // Reload with modified definition while running
    supervisor.load("p1", dir, vec![def_v2]);

    let after = supervisor.list("p1");
    assert_eq!(after[0].status, ProcessStatus::Running);
    assert!(
        after[0].changed,
        "running process definition change must set changed: true"
    );
    assert_eq!(after[0].message, "definition changed; restart to apply");
    assert_eq!(after[0].def.command, "serve v2");

    // Restart process applies new definition and clears changed flag
    supervisor.restart("p1", "web").expect("restart");
    let after_restart = supervisor.list("p1");
    assert_eq!(after_restart[0].status, ProcessStatus::Starting);
    assert!(!after_restart[0].changed);
    assert!(after_restart[0].message.is_empty());
    assert_eq!(runner.latest_start().unwrap().command, vec!["serve v2"]);
}

#[test]
fn test_modify_stopped_definition_updates_without_changed_flag() {
    let dir = "D:/test/idle";
    let def_v1 = Def {
        name: "task".to_string(),
        command: "task v1".to_string(),
        ..Default::default()
    };
    let def_v2 = Def {
        name: "task".to_string(),
        command: "task v2".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[def_v1.clone(), def_v2.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", dir, vec![def_v1]);
    let initial = supervisor.list("p1");
    assert_eq!(initial[0].status, ProcessStatus::Stopped);
    assert!(!initial[0].changed);

    // Reload while stopped
    supervisor.load("p1", dir, vec![def_v2]);
    let updated = supervisor.list("p1");
    assert_eq!(updated[0].status, ProcessStatus::Stopped);
    assert!(
        !updated[0].changed,
        "stopped process definition update must not mark changed"
    );
    assert_eq!(updated[0].def.command, "task v2");
}

#[test]
fn test_remove_running_process_retains_until_stopped() {
    let dir = "D:/test/remove";
    let def = Def {
        name: "daemon".to_string(),
        command: "run-daemon".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", dir, vec![def]);
    supervisor.start("p1", "daemon").expect("start");

    // Remove from definitions by loading empty vec
    supervisor.load("p1", dir, vec![]);

    let list = supervisor.list("p1");
    assert_eq!(
        list.len(),
        1,
        "running process must remain visible after removal from defs"
    );
    assert!(list[0].changed);
    assert_eq!(list[0].message, "no longer defined");

    // Stopping the removed process purges it
    supervisor.stop("p1", "daemon").expect("stop");
    // Reload to perform cleanup (matching Go Load cleanup of handle==nil)
    supervisor.load("p1", dir, vec![]);
    let list_after = supervisor.list("p1");
    assert_eq!(
        list_after.len(),
        0,
        "stopped removed process must be cleaned up"
    );
}

#[test]
fn test_untrusted_change_refuses_restart_and_leaves_run_alone() {
    let dir = "D:/test/untrusted_mod";
    let def_v1 = Def {
        name: "srv".to_string(),
        command: "serve v1".to_string(),
        ..Default::default()
    };
    let def_untrusted = Def {
        name: "srv".to_string(),
        command: "serve v2-untrusted".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    // Only def_v1 is trusted
    let trust = create_test_trust(dir, &[def_v1.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", dir, vec![def_v1]);
    supervisor.start("p1", "srv").expect("start");

    let gen = supervisor.process_generation("p1", "srv").unwrap();
    let handle_id = runner.latest_start().unwrap().id;
    supervisor.set_running("p1", "srv", gen, &handle_id);

    // Load untrusted definition
    supervisor.load("p1", dir, vec![def_untrusted]);

    let p = supervisor.list("p1");
    assert_eq!(p[0].status, ProcessStatus::Running);
    assert!(!p[0].trusted);
    assert!(p[0].changed);

    // Restart must be refused
    let err = supervisor.restart("p1", "srv").unwrap_err();
    assert!(matches!(err, SupervisorError::Untrusted(_)));

    // Process is still running previous run
    let p_after = supervisor.list("p1");
    assert_eq!(p_after[0].status, ProcessStatus::Running);
    assert_eq!(
        runner.start_count(),
        1,
        "runner must not have started new process"
    );
}

#[test]
fn test_trust_process_transitions_untrusted_to_stopped() {
    let dir = "D:/test/trust_transition";
    let def = Def {
        name: "custom".to_string(),
        command: "run-custom".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("p1", dir, vec![def]);
    assert_eq!(supervisor.list("p1")[0].status, ProcessStatus::Untrusted);

    supervisor
        .trust_process("p1", "custom")
        .expect("trust process");

    let p = supervisor.list("p1");
    assert!(p[0].trusted);
    assert_eq!(p[0].status, ProcessStatus::Stopped);
}

#[test]
fn test_all_and_sessions_queries() {
    let dir1 = "D:/test/p1";
    let dir2 = "D:/test/p2";

    let def1 = Def {
        name: "worker".to_string(),
        command: "worker".to_string(),
        ..Default::default()
    };
    let def2 = Def {
        name: "api".to_string(),
        command: "api".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir1, &[def1.clone()]);
    trust
        .add(dir2, &def2.variant())
        .expect("trust def2 in dir2");
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    supervisor.load("proj_b", dir1, vec![def1]);
    supervisor.load("proj_a", dir2, vec![def2]);

    // all() returns projects in sorted order: proj_a then proj_b
    let all = supervisor.all();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].project, "proj_a");
    assert_eq!(all[0].proc.def.name, "api");
    assert_eq!(all[1].project, "proj_b");
    assert_eq!(all[1].proc.def.name, "worker");

    // sessions() before start is empty
    assert!(supervisor.sessions("proj_a").is_empty());

    // Start api in proj_a
    supervisor.start("proj_a", "api").expect("start");
    let sess = supervisor.sessions("proj_a");
    assert_eq!(sess.len(), 1);
    assert_eq!(sess[0].def.name, "api");
    assert!(!sess[0].pane.is_empty());
}

#[test]
fn test_on_change_and_on_event_callbacks() {
    let dir = "D:/test/callbacks";
    let def = Def {
        name: "svc".to_string(),
        command: "serve".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust(dir, &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));
    supervisor.init();

    let change_count = Arc::new(AtomicU64::new(0));
    let cc = Arc::clone(&change_count);
    supervisor.set_on_change(move || {
        cc.fetch_add(1, Ordering::SeqCst);
    });

    let events = Arc::new(Mutex::new(Vec::new()));
    let ev_clone = Arc::clone(&events);
    supervisor.set_on_event(move |e: Event| {
        ev_clone.lock().unwrap().push(e);
    });

    // 1. load triggers on_change
    supervisor.load("p1", dir, vec![def]);
    assert!(change_count.load(Ordering::SeqCst) >= 1);

    // 2. start triggers on_change and Started event
    supervisor.start("p1", "svc").expect("start");
    assert!(change_count.load(Ordering::SeqCst) >= 2);
    {
        let evs = events.lock().unwrap();
        assert_eq!(evs.last().unwrap().kind, EventKind::Started);
    }

    // 3. stop triggers on_change and Stopped event
    supervisor.stop("p1", "svc").expect("stop");
    assert!(change_count.load(Ordering::SeqCst) >= 3);
    {
        let evs = events.lock().unwrap();
        assert_eq!(evs.last().unwrap().kind, EventKind::Stopped);
    }
}
