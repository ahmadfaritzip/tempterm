//! P1-05 Integration Tests: Filesystem Watching and Auto-Reload
//! Corresponds to Go `tempterm/internal/procs/watch_test.go` and `tempterm/service/discover_test.go`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use temp_terminal_procs::def::Def;
use temp_terminal_procs::supervisor::{
    Handle, ProcessStatus, Runner, RunnerError, StartOptions, Supervisor,
};
use temp_terminal_procs::trust::Trust;
use temp_terminal_procs::watch::{make_ignore_filter, poll_files, WatchError};

fn tempfile_path(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "{}_{}_{}_{}.tmp",
        prefix,
        std::process::id(),
        nanos,
        count
    ))
}

struct MockHandle {
    id: String,
    history: Vec<u8>,
}

impl Handle for MockHandle {
    fn id(&self) -> &str {
        &self.id
    }
    fn pid(&self) -> u32 {
        1234
    }
    fn history(&self) -> Vec<u8> {
        self.history.clone()
    }
    fn is_alive(&self) -> bool {
        true
    }
}

struct MockRunner {
    starts: AtomicUsize,
    latest_options: Mutex<Option<StartOptions>>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            starts: AtomicUsize::new(0),
            latest_options: Mutex::new(None),
        }
    }

    fn count(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }

    fn latest(&self) -> Option<StartOptions> {
        self.latest_options.lock().unwrap().clone()
    }
}

impl Runner for MockRunner {
    fn start(&self, options: StartOptions) -> Result<Arc<dyn Handle>, RunnerError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let id = options.id.clone();
        let history = if options.history.is_empty() {
            b"old scrollback".to_vec()
        } else {
            options.history.clone()
        };
        *self.latest_options.lock().unwrap() = Some(options);
        Ok(Arc::new(MockHandle { id, history }))
    }

    fn get(&self, _id: &str) -> Option<Arc<dyn Handle>> {
        None
    }

    fn remove(&self, _id: &str) {}
}

/// Fake watch implementation matching Go `fakeWatch` in `tempterm/internal/procs/watch_test.go:16-58`.
struct FakeWatch {
    send_tx: tokio::sync::broadcast::Sender<Vec<String>>,
    cancel_flags: Mutex<Vec<Arc<AtomicBool>>>,
    fail_with_error: Mutex<Option<String>>,
}

impl FakeWatch {
    fn new() -> Arc<Self> {
        let (tx, _rx) = tokio::sync::broadcast::channel(64);
        Arc::new(Self {
            send_tx: tx,
            cancel_flags: Mutex::new(Vec::new()),
            fail_with_error: Mutex::new(None),
        })
    }

    fn send(&self, paths: &[&str]) {
        let string_paths: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        let _ = self.send_tx.send(string_paths);
    }

    fn last_cancel_flag(&self) -> Option<Arc<AtomicBool>> {
        self.cancel_flags.lock().unwrap().last().cloned()
    }

    fn set_error(&self, err: &str) {
        *self.fail_with_error.lock().unwrap() = Some(err.to_string());
    }

    fn into_watch_func(self: &Arc<Self>) -> temp_terminal_procs::watch::WatchFunc {
        let this = Arc::clone(self);
        Arc::new(move |_root, cancel, changed_cb| {
            if let Some(err_msg) = this.fail_with_error.lock().unwrap().clone() {
                return Err(WatchError::Failed(err_msg));
            }

            this.cancel_flags.lock().unwrap().push(Arc::clone(&cancel));
            let mut receiver = this.send_tx.subscribe();

            while !cancel.load(Ordering::SeqCst) {
                match receiver.try_recv() {
                    Ok(paths) => {
                        changed_cb(paths);
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        break;
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
                }
            }

            Ok(())
        })
    }
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
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

#[tokio::test]
async fn test_watch_takes_changes_and_ignores_the_rest() {
    let dir = tempfile_path("watch_dir_1");
    fs::create_dir_all(&dir).unwrap();
    let trust_path = dir.join("trust.json");
    let trust = Trust::open(trust_path.to_str().unwrap());

    let def = Def {
        name: "web".to_string(),
        command: "echo hi".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    trust.add(dir.to_str().unwrap(), &def.variant()).unwrap();

    let runner = Arc::new(MockRunner::new());
    let fake_watch = FakeWatch::new();

    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_watch_limits(Duration::from_millis(20), Duration::from_millis(20))
            .with_readiness_limits(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(5),
            ),
    );
    supervisor.init();
    supervisor.set_watch_fn(fake_watch.into_watch_func());

    supervisor.load("p", dir.to_str().unwrap(), vec![def]);
    println!("BEFORE START");
    supervisor.start("p", "web").expect("start process");
    println!("AFTER START, WAITING FOR RUNNING");

    // Wait until process reaches Running state
    assert!(
        wait_for_condition(Duration::from_millis(500), || {
            let list = supervisor.list("p");
            !list.is_empty() && list[0].status == ProcessStatus::Running
        })
        .await
    );
    println!("PROCESS IS RUNNING");

    assert_eq!(runner.count(), 1);

    // Send a change to a path NOT matching globs ("README.md")
    fake_watch.send(&["README.md"]);

    // Sleep to give debounce window time to elapse
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify it DID NOT restart the process
    assert_eq!(runner.count(), 1);
    println!("VERIFIED RUNNER COUNT IS 1");

    supervisor.close();
    println!("SUPERVISOR CLOSED");

    let _ = fs::remove_dir_all(&dir);
    println!("TEST COMPLETED");
}

#[tokio::test]
async fn test_watch_restarts_on_a_change() {
    let dir = tempfile_path("watch_dir_2");
    fs::create_dir_all(&dir).unwrap();
    let trust_path = dir.join("trust.json");
    let trust = Trust::open(trust_path.to_str().unwrap());

    let def = Def {
        name: "web".to_string(),
        command: "echo hi".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    trust.add(dir.to_str().unwrap(), &def.variant()).unwrap();

    let runner = Arc::new(MockRunner::new());
    let fake_watch = FakeWatch::new();

    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_watch_limits(Duration::from_millis(20), Duration::from_millis(20))
            .with_readiness_limits(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(5),
            ),
    );
    supervisor.init();
    supervisor.set_watch_fn(fake_watch.into_watch_func());

    supervisor.load("p", dir.to_str().unwrap(), vec![def]);
    println!("TEST 2: BEFORE START");
    supervisor.start("p", "web").expect("start process");
    println!("TEST 2: WAITING FOR RUNNING");

    assert!(
        wait_for_condition(Duration::from_millis(500), || {
            let list = supervisor.list("p");
            !list.is_empty() && list[0].status == ProcessStatus::Running
        })
        .await
    );
    println!("TEST 2: RUNNING, SENDING CHANGE");

    // Send a change to a path matching the watch glob
    fake_watch.send(&["src/a.rs"]);
    println!("TEST 2: CHANGE SENT, WAITING FOR RUNNER COUNT == 2");

    // Wait for the restart to trigger
    assert!(
        wait_for_condition(Duration::from_secs(1), || runner.count() == 2).await,
        "process must restart once on watch change"
    );
    println!("TEST 2: RESTART TRIGGERED");

    // Check process metadata
    let procs = supervisor.list("p");
    assert_eq!(procs[0].last_change, "src/a.rs");
    assert_eq!(
        procs[0].restarts, 0,
        "watch restart does not count in crash restart backoff"
    );

    // Check banner in history
    let opts = runner.latest().expect("latest options");
    let history_str = String::from_utf8_lossy(&opts.history);
    assert!(
        history_str.contains("── restarted (changed: src/a.rs) ──"),
        "expected restart banner with changed file path, got: {}",
        history_str
    );
    assert!(
        history_str.contains("old scrollback"),
        "previous scrollback history must be preserved"
    );

    let _ = fs::remove_dir_all(&dir);
    println!("TEST 2: DIR CLEANED");
    supervisor.close();
    println!("TEST 2: SUPERVISOR CLOSED");
}

#[tokio::test]
async fn test_watch_restarts_once_per_burst() {
    let dir = tempfile_path("watch_dir_3");
    fs::create_dir_all(&dir).unwrap();
    let trust_path = dir.join("trust.json");
    let trust = Trust::open(trust_path.to_str().unwrap());

    let def = Def {
        name: "web".to_string(),
        command: "echo hi".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    trust.add(dir.to_str().unwrap(), &def.variant()).unwrap();

    let runner = Arc::new(MockRunner::new());
    let fake_watch = FakeWatch::new();

    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_watch_limits(Duration::from_millis(40), Duration::from_millis(20))
            .with_readiness_limits(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(5),
            ),
    );
    supervisor.init();
    supervisor.set_watch_fn(fake_watch.into_watch_func());

    supervisor.load("p", dir.to_str().unwrap(), vec![def]);
    supervisor.start("p", "web").expect("start process");

    assert!(
        wait_for_condition(Duration::from_millis(500), || {
            let list = supervisor.list("p");
            !list.is_empty() && list[0].status == ProcessStatus::Running
        })
        .await
    );

    // Send multiple rapid changes in a burst within debounce window
    fake_watch.send(&["src/a.rs"]);
    fake_watch.send(&["src/b.rs"]);
    fake_watch.send(&["src/c.rs"]);

    // Allow debounce to fire and settle
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Must restart exactly once for the entire burst (count 1 -> 2)
    assert_eq!(runner.count(), 2, "burst of 3 files must restart only once");

    supervisor.close();
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_stop_ends_the_watch() {
    let dir = tempfile_path("watch_dir_4");
    fs::create_dir_all(&dir).unwrap();
    let trust_path = dir.join("trust.json");
    let trust = Trust::open(trust_path.to_str().unwrap());

    let def = Def {
        name: "web".to_string(),
        command: "echo hi".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    trust.add(dir.to_str().unwrap(), &def.variant()).unwrap();

    let runner = Arc::new(MockRunner::new());
    let fake_watch = FakeWatch::new();

    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_watch_limits(Duration::from_millis(20), Duration::from_millis(20))
            .with_readiness_limits(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(5),
            ),
    );
    supervisor.init();
    supervisor.set_watch_fn(fake_watch.into_watch_func());

    supervisor.load("p", dir.to_str().unwrap(), vec![def]);
    supervisor.start("p", "web").expect("start process");

    assert!(
        wait_for_condition(Duration::from_millis(500), || {
            let list = supervisor.list("p");
            !list.is_empty() && list[0].status == ProcessStatus::Running
        })
        .await
    );

    let cancel_flag = fake_watch
        .last_cancel_flag()
        .expect("must have active cancel flag");
    assert!(!cancel_flag.load(Ordering::SeqCst));

    // Stop process
    supervisor.stop("p", "web").expect("stop process");

    // Watch cancel flag must be signaled
    assert!(
        cancel_flag.load(Ordering::SeqCst),
        "stop must cancel active watch"
    );

    supervisor.close();
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_watch_failure_falls_back_to_polling() {
    let dir = tempfile_path("watch_dir_5");
    fs::create_dir_all(&dir).unwrap();
    let trust_path = dir.join("trust.json");
    let trust = Trust::open(trust_path.to_str().unwrap());

    let def = Def {
        name: "web".to_string(),
        command: "echo hi".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    trust.add(dir.to_str().unwrap(), &def.variant()).unwrap();

    let runner = Arc::new(MockRunner::new());
    let fake_watch = FakeWatch::new();
    fake_watch.set_error("no watch on this system");

    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_watch_limits(Duration::from_millis(20), Duration::from_millis(30))
            .with_readiness_limits(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(5),
            ),
    );
    supervisor.init();
    supervisor.set_watch_fn(fake_watch.into_watch_func());

    supervisor.load("p", dir.to_str().unwrap(), vec![def]);
    supervisor.start("p", "web").expect("start process");

    // Check that watch failure sets message but keeps process running
    assert!(
        wait_for_condition(Duration::from_millis(500), || {
            let list = supervisor.list("p");
            !list.is_empty()
                && list[0].status == ProcessStatus::Running
                && list[0]
                    .message
                    .contains("watch failed: no watch on this system")
        })
        .await
    );

    assert_eq!(
        runner.count(),
        1,
        "watch failure must not abort process start"
    );

    // Create src directory and write file to test polling detection
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    fs::write(src_dir.join("a.rs"), b"mod test;").unwrap();

    // Verify polling triggers restart
    assert!(
        wait_for_condition(Duration::from_secs(2), || runner.count() == 2).await,
        "polling fallback must detect new file and restart process"
    );

    let procs = supervisor.list("p");
    assert_eq!(procs[0].last_change, "src/a.rs");

    supervisor.close();
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_watch_restart_waits_for_trust() {
    let dir = tempfile_path("watch_dir_6");
    fs::create_dir_all(&dir).unwrap();
    let trust_path = dir.join("trust.json");
    let trust = Trust::open(trust_path.to_str().unwrap());

    let def = Def {
        name: "web".to_string(),
        command: "echo hi".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    trust.add(dir.to_str().unwrap(), &def.variant()).unwrap();

    let runner = Arc::new(MockRunner::new());
    let fake_watch = FakeWatch::new();

    let supervisor = Arc::new(
        Supervisor::new(runner.clone(), Some(trust))
            .with_watch_limits(Duration::from_millis(20), Duration::from_millis(20))
            .with_readiness_limits(
                Duration::from_millis(10),
                Duration::from_millis(5),
                Duration::from_millis(5),
            ),
    );
    supervisor.init();
    supervisor.set_watch_fn(fake_watch.into_watch_func());

    supervisor.load("p", dir.to_str().unwrap(), vec![def]);
    supervisor.start("p", "web").expect("start process");

    assert!(
        wait_for_condition(Duration::from_millis(500), || {
            let list = supervisor.list("p");
            !list.is_empty() && list[0].status == ProcessStatus::Running
        })
        .await
    );

    // Modify definition without trusting it
    let changed_def = Def {
        name: "web".to_string(),
        command: "echo changed_untrusted".to_string(),
        watch: vec!["src/**".to_string()],
        ..Default::default()
    };
    supervisor.load("p", dir.to_str().unwrap(), vec![changed_def]);

    // Send file change
    fake_watch.send(&["src/a.rs"]);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Process must NOT restart because definition is untrusted
    assert_eq!(runner.count(), 1);

    let procs = supervisor.list("p");
    assert_eq!(procs[0].status, ProcessStatus::Running);
    assert!(
        procs[0].message.contains("trust"),
        "expected trust warning in message, got: {}",
        procs[0].message
    );

    supervisor.close();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_ignore_filter_and_poll_files() {
    let dir = tempfile_path("ignore_filter_test");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("dist")).unwrap();
    fs::create_dir_all(dir.join("node_modules")).unwrap();
    fs::create_dir_all(dir.join(".git")).unwrap();

    fs::write(dir.join(".gitignore"), b"dist/\n*.log\n").unwrap();
    fs::write(dir.join("src/a.rs"), b"fn a() {}").unwrap();
    fs::write(dir.join("src/a.log"), b"log").unwrap();
    fs::write(dir.join("dist/bundle.js"), b"code").unwrap();
    fs::write(dir.join("node_modules/dep.js"), b"dep").unwrap();
    fs::write(dir.join(".git/config"), b"git").unwrap();
    fs::write(dir.join("root.txt"), b"root").unwrap();

    let filter = make_ignore_filter(&dir);
    assert!(filter("dist/bundle.js", false));
    assert!(filter("dist", true));
    assert!(filter("node_modules/dep.js", false));
    assert!(filter("node_modules", true));
    assert!(filter(".git/config", false));
    assert!(filter("src/a.log", false));
    assert!(!filter("src/a.rs", false));
    assert!(!filter("root.txt", false));

    let matched = poll_files(
        &dir,
        &["src/**".to_string(), "*.txt".to_string()],
        filter.as_ref(),
    );
    assert_eq!(
        matched,
        vec!["root.txt".to_string(), "src/a.rs".to_string()]
    );

    let _ = fs::remove_dir_all(&dir);
}
