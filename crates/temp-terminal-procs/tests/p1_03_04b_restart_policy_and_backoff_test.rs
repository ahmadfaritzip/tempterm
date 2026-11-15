//! Integration tests for P1-03 Step 4B: Restart Policy, Exponential Backoff, and Rate Limiting.
//! Corresponds to Go `tempterm/internal/procs/supervisor_test.go:135-248`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use temp_terminal_procs::def::Def;
use temp_terminal_procs::restart::{
    calculate_backoff, format_manual_restart_banner, format_restart_banner, should_restart,
    update_restart_window,
};
use temp_terminal_procs::supervisor::{
    EventKind, Handle, ProcessStatus, Runner, RunnerError, StartOptions, Supervisor,
};
use temp_terminal_procs::trust::Trust;

#[derive(Default)]
struct MockRunner {
    starts: Mutex<Vec<StartOptions>>,
    handles: Mutex<HashMap<String, Arc<MockHandle>>>,
}

impl MockRunner {
    fn new() -> Self {
        Self::default()
    }

    fn starts_count(&self) -> usize {
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
            pid: 1234,
            history: Mutex::new(b"initial output".to_vec()),
        });
        self.starts.lock().unwrap().push(options.clone());
        self.handles
            .lock()
            .unwrap()
            .insert(options.id, handle.clone());
        Ok(handle)
    }

    fn get(&self, id: &str) -> Option<Arc<dyn Handle>> {
        self.handles
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .map(|h| h as Arc<dyn Handle>)
    }

    fn remove(&self, id: &str) {
        self.handles.lock().unwrap().remove(id);
    }
}

struct MockHandle {
    id: String,
    pid: u32,
    history: Mutex<Vec<u8>>,
}

impl Handle for MockHandle {
    fn id(&self) -> &str {
        &self.id
    }

    fn pid(&self) -> u32 {
        self.pid
    }

    fn history(&self) -> Vec<u8> {
        self.history.lock().unwrap().clone()
    }

    fn is_alive(&self) -> bool {
        self.pid != 0
    }
}

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

// -----------------------------------------------------------------------------
// 1. Policy Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_restart_policy_never_and_empty() {
    assert!(!should_restart("", 0));
    assert!(!should_restart("", 1));
    assert!(!should_restart("never", 0));
    assert!(!should_restart("never", 1));

    let def = Def {
        name: "proc-never".to_string(),
        command: "echo test".to_string(),
        restart: "never".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "proc-never").expect("start");

    let gen = supervisor.process_generation("p1", "proc-never").unwrap();
    supervisor
        .handle_exit_async("p1", "proc-never", gen, 1)
        .await;

    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Failed);
    assert_eq!(p.last_exit, 1);
    assert_eq!(p.restarts, 0);
    assert_eq!(runner.starts_count(), 1); // No new restart
}

#[tokio::test]
async fn test_restart_policy_on_failure() {
    assert!(!should_restart("on-failure", 0));
    assert!(should_restart("on-failure", 1));
    assert!(should_restart("on-failure", 137));

    let def = Def {
        name: "proc-fail".to_string(),
        command: "echo test".to_string(),
        restart: "on-failure".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::ZERO,
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "proc-fail").expect("start");

    let gen = supervisor.process_generation("p1", "proc-fail").unwrap();

    // Exit code 0 -> should NOT restart
    supervisor
        .handle_exit_async("p1", "proc-fail", gen, 0)
        .await;
    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Exited);
    assert_eq!(runner.starts_count(), 1);

    // Manual start again
    supervisor.start("p1", "proc-fail").expect("start");
    let gen2 = supervisor.process_generation("p1", "proc-fail").unwrap();

    // Exit code 1 -> should restart
    supervisor
        .handle_exit_async("p1", "proc-fail", gen2, 1)
        .await;
    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Starting);
    assert_eq!(p.restarts, 1);
    assert_eq!(runner.starts_count(), 3); // initial + manual start + restart
}

#[tokio::test]
async fn test_restart_policy_always() {
    assert!(should_restart("always", 0));
    assert!(should_restart("always", 1));

    let def = Def {
        name: "proc-always".to_string(),
        command: "echo test".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::ZERO,
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "proc-always").expect("start");

    let gen = supervisor.process_generation("p1", "proc-always").unwrap();

    // Exit code 0 on always -> restarts!
    supervisor
        .handle_exit_async("p1", "proc-always", gen, 0)
        .await;
    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Starting);
    assert_eq!(p.restarts, 1);
    assert_eq!(runner.starts_count(), 2);
}

// -----------------------------------------------------------------------------
// 2. Rate Limiting Tests (Ninth vs Tenth Boundary)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_rate_limiting_exact_boundary_ninth_and_tenth() {
    let def = Def {
        name: "crasher".to_string(),
        command: "crash".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::ZERO,
    ));
    supervisor.set_on_event(move |e| {
        events_clone.lock().unwrap().push(e);
    });

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "crasher").expect("start");

    // We do 8 restarts (starts_count goes from 1 up to 9)
    for _ in 1..=8 {
        let gen = supervisor.process_generation("p1", "crasher").unwrap();
        supervisor.handle_exit_async("p1", "crasher", gen, 1).await;
    }

    // After 8 restarts:
    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.restarts, 8);
    assert_eq!(runner.starts_count(), 9);

    // 9th restart request:
    let gen9 = supervisor.process_generation("p1", "crasher").unwrap();
    supervisor.handle_exit_async("p1", "crasher", gen9, 1).await;
    let p9 = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p9.restarts, 9);
    assert_eq!(p9.status, ProcessStatus::Starting);
    assert_eq!(runner.starts_count(), 10); // 1 initial + 9 restarts

    // 10th restart request: must trigger Paused and NOT launch 10th restart!
    let gen10 = supervisor.process_generation("p1", "crasher").unwrap();
    supervisor
        .handle_exit_async("p1", "crasher", gen10, 1)
        .await;
    let p10 = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p10.restarts, 10);
    assert_eq!(p10.status, ProcessStatus::Paused);
    assert_eq!(p10.message, "restart paused after 10 restarts in a minute");
    assert_eq!(runner.starts_count(), 10); // No 11th start!

    // Verify EventPaused was emitted
    let evs = events.lock().unwrap();
    assert!(evs.iter().any(|e| e.kind == EventKind::Paused));
}

#[test]
fn test_sliding_window_exact_60s_retained_and_older_pruned() {
    let now = 100_000u64;
    let window = Duration::from_secs(60);

    let ts_older = now - 60_001; // > 60s
    let ts_exact = now - 60_000; // == 60s
    let ts_recent = now - 30_000; // < 60s

    let timestamps = vec![ts_older, ts_exact, ts_recent];
    let kept = update_restart_window(&timestamps, now, window);

    assert_eq!(kept, vec![ts_exact, ts_recent, now]);
    assert_eq!(kept.len(), 3);
}

// -----------------------------------------------------------------------------
// 3. Backoff Tests
// -----------------------------------------------------------------------------

#[test]
fn test_backoff_delay_sequence_and_cap() {
    let base = Duration::from_secs(1);
    assert_eq!(calculate_backoff(1, base), Duration::from_secs(1));
    assert_eq!(calculate_backoff(2, base), Duration::from_secs(2));
    assert_eq!(calculate_backoff(3, base), Duration::from_secs(4));
    assert_eq!(calculate_backoff(4, base), Duration::from_secs(8));
    assert_eq!(calculate_backoff(5, base), Duration::from_secs(16));
    assert_eq!(calculate_backoff(6, base), Duration::from_secs(30)); // 32s capped at 30s
    assert_eq!(calculate_backoff(7, base), Duration::from_secs(30));
    assert_eq!(calculate_backoff(8, base), Duration::from_secs(30));
    assert_eq!(calculate_backoff(9, base), Duration::from_secs(30));
}

#[tokio::test]
async fn test_manual_start_and_restart_resets_restart_history() {
    let def = Def {
        name: "worker".to_string(),
        command: "work".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::ZERO,
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "worker").expect("start");

    // Perform 3 restarts
    for _ in 0..3 {
        let gen = supervisor.process_generation("p1", "worker").unwrap();
        supervisor.handle_exit_async("p1", "worker", gen, 1).await;
    }

    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.restarts, 3);

    // If the process is already running, stop it first before manual start,
    // or call stop() then start() (or restart())
    supervisor.stop("p1", "worker").expect("stop");
    supervisor.start("p1", "worker").expect("start");
    let p_after_start = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p_after_start.restarts, 0);

    // Perform another restart -> count becomes 1, not 4
    let gen_new = supervisor.process_generation("p1", "worker").unwrap();
    supervisor
        .handle_exit_async("p1", "worker", gen_new, 1)
        .await;
    let p_after_restart = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p_after_restart.restarts, 1);

    // Manual restart also resets
    supervisor.restart("p1", "worker").expect("restart");
    let p_after_manual_restart = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p_after_manual_restart.restarts, 0);
}

// -----------------------------------------------------------------------------
// 4. Cancellation & Concurrency Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_stop_during_backoff_cancels_restart() {
    let def = Def {
        name: "cancellee".to_string(),
        command: "sleep".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    // Set 50ms delay
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::from_millis(50),
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "cancellee").expect("start");

    let gen = supervisor.process_generation("p1", "cancellee").unwrap();

    // Spawn exit in background task which will wait 50ms before restarting
    let sup_clone = supervisor.clone();
    let handle = tokio::spawn(async move {
        sup_clone.handle_exit_async("p1", "cancellee", gen, 1).await;
    });

    // In the meantime, call stop()
    tokio::time::sleep(Duration::from_millis(10)).await;
    supervisor.stop("p1", "cancellee").expect("stop");

    handle.await.unwrap();

    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Stopped);
    assert_eq!(runner.starts_count(), 1); // No second start!
}

#[tokio::test]
async fn test_manual_start_during_backoff_drops_stale_timer() {
    let def = Def {
        name: "stale-timer".to_string(),
        command: "sleep".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::from_millis(50),
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "stale-timer").expect("start");

    let gen = supervisor.process_generation("p1", "stale-timer").unwrap();

    // Exit starts timer for generation 1
    let sup_clone = supervisor.clone();
    let handle = tokio::spawn(async move {
        sup_clone
            .handle_exit_async("p1", "stale-timer", gen, 1)
            .await;
    });

    // Manual start advances generation to 2
    tokio::time::sleep(Duration::from_millis(10)).await;
    supervisor.start("p1", "stale-timer").expect("start");
    assert_eq!(runner.starts_count(), 2);

    handle.await.unwrap();

    // Gen 1 timer should be dropped, so starts_count remains 2
    assert_eq!(runner.starts_count(), 2);
    let gen_now = supervisor.process_generation("p1", "stale-timer").unwrap();
    assert_eq!(gen_now, 2);
}

#[tokio::test]
async fn test_trust_revocation_during_backoff_transitions_to_untrusted() {
    let def = Def {
        name: "untrust-me".to_string(),
        command: "cmd".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::from_millis(50),
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "untrust-me").expect("start");

    let gen = supervisor.process_generation("p1", "untrust-me").unwrap();

    let sup_clone = supervisor.clone();
    let handle = tokio::spawn(async move {
        sup_clone
            .handle_exit_async("p1", "untrust-me", gen, 1)
            .await;
    });

    // During backoff, reload with a new untrusted variant
    tokio::time::sleep(Duration::from_millis(10)).await;
    let def_untrusted = Def {
        name: "untrust-me".to_string(),
        command: "new cmd untrusted".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };
    supervisor.load("p1", "D:/project/app", vec![def_untrusted]);

    handle.await.unwrap();

    let p = supervisor.list("p1").into_iter().next().unwrap();
    assert_eq!(p.status, ProcessStatus::Untrusted);
    assert_eq!(p.message, "the changed definition isn't trusted yet");
    assert_eq!(runner.starts_count(), 1); // Not restarted
}

#[tokio::test]
async fn test_supervisor_close_cancels_restart() {
    let def = Def {
        name: "closer".to_string(),
        command: "cmd".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::from_millis(50),
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "closer").expect("start");

    let gen = supervisor.process_generation("p1", "closer").unwrap();

    let sup_clone = supervisor.clone();
    let handle = tokio::spawn(async move {
        sup_clone.handle_exit_async("p1", "closer", gen, 1).await;
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    supervisor.close();

    handle.await.unwrap();

    assert_eq!(runner.starts_count(), 1);
}

// -----------------------------------------------------------------------------
// 5. History Formatting & Propagation Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_restart_history_banner_propagation() {
    let def = Def {
        name: "historian".to_string(),
        command: "cmd".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)).with_limits(
        Duration::from_secs(60),
        10,
        Duration::ZERO,
    ));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "historian").expect("start");

    let gen = supervisor.process_generation("p1", "historian").unwrap();
    supervisor
        .handle_exit_async("p1", "historian", gen, 42)
        .await;

    let latest_opts = runner.latest_start().unwrap();
    let expected_banner = format_restart_banner(42);

    assert!(latest_opts.history.starts_with(b"initial output"));
    assert!(latest_opts.history.ends_with(&expected_banner));
}

#[tokio::test]
async fn test_manual_restart_history_banner() {
    let def = Def {
        name: "manual-hist".to_string(),
        command: "cmd".to_string(),
        restart: "never".to_string(),
        ..Default::default()
    };

    let runner = Arc::new(MockRunner::new());
    let trust = create_test_trust("D:/project/app", &[def.clone()]);
    let supervisor = Arc::new(Supervisor::new(runner.clone(), Some(trust)));

    supervisor.load("p1", "D:/project/app", vec![def]);
    supervisor.start("p1", "manual-hist").expect("start");

    supervisor.restart("p1", "manual-hist").expect("restart");

    let latest_opts = runner.latest_start().unwrap();
    let expected_banner = format_manual_restart_banner();

    assert!(latest_opts.history.starts_with(b"initial output"));
    assert!(latest_opts.history.ends_with(&expected_banner));
}
