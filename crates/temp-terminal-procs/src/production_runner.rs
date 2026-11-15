//! Production Process Runner integrating OS processes and Windows ConPTY
//! with the Supervisor Runner and Handle contracts.
//!
//! Corresponds to Go `tempterm/internal/daemon/process.go` (`daemonRunner`)
//! and `tempterm/internal/session/registry.go` (`Registry.Start`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use temp_terminal_pty::{PtyOptions, PtyProc};

use crate::supervisor::{Handle, Runner, RunnerError, StartOptions};

/// Maximum terminal scrollback history stored in memory ring buffer per pane (2MB).
const MAX_HISTORY_BYTES: usize = 2 * 1024 * 1024;

/// Exit callback type for supervisor notification.
pub type ExitNotifier = Arc<dyn Fn(&str, i32) + Send + Sync>;

/// Live output callback type for streaming terminal consumers.
pub type OutputSubscriber = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;

fn dirs_sys_home() -> Option<String> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
}

/// Real operating system process handle backed by ConPTY.
pub struct ProductionHandle {
    id: String,
    pid: u32,
    pty: PtyProc,
    history: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
    subscribers: Arc<Mutex<Vec<OutputSubscriber>>>,
}

impl ProductionHandle {
    pub fn pty(&self) -> &PtyProc {
        &self.pty
    }

    pub fn write_in(&self, buf: &[u8]) -> std::io::Result<usize> {
        self.pty.write_in(buf)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), temp_terminal_pty::PtyError> {
        self.pty.resize(cols, rows)
    }

    /// Subscribes a listener to live output streaming for this handle.
    pub fn subscribe<F>(&self, callback: F)
    where
        F: Fn(&str, &[u8]) + Send + Sync + 'static,
    {
        self.subscribers.lock().unwrap().push(Arc::new(callback));
    }
}

impl Handle for ProductionHandle {
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
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.pty.is_alive()
    }
}

/// Production runner creating real OS processes inside Windows ConPTY.
pub struct ProductionRunner {
    handles: Mutex<HashMap<String, Arc<ProductionHandle>>>,
    on_exit: Mutex<Option<ExitNotifier>>,
    on_output: Mutex<Option<OutputSubscriber>>,
}

impl ProductionRunner {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
            on_exit: Mutex::new(None),
            on_output: Mutex::new(None),
        }
    }

    /// Sets the exit notification handler called when a child process exits.
    pub fn set_on_exit<F>(&self, notifier: F)
    where
        F: Fn(&str, i32) + Send + Sync + 'static,
    {
        *self.on_exit.lock().unwrap() = Some(Arc::new(notifier));
    }

    /// Sets a global live output callback for all managed processes.
    pub fn set_on_output<F>(&self, subscriber: F)
    where
        F: Fn(&str, &[u8]) + Send + Sync + 'static,
    {
        *self.on_output.lock().unwrap() = Some(Arc::new(subscriber));
    }

    /// Returns the specific typed `ProductionHandle` for interactive operations (write_in, resize, subscribe).
    pub fn get_production_handle(&self, id: &str) -> Option<Arc<ProductionHandle>> {
        self.handles.lock().unwrap().get(id).cloned()
    }
}

impl Default for ProductionRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner for ProductionRunner {
    fn start(&self, options: StartOptions) -> Result<Arc<dyn Handle>, RunnerError> {
        if options.command.is_empty() {
            return Err(RunnerError::StartFailed("empty command".to_string()));
        }

        // Determine executable path and args
        // Matches Go commandLine: runs command via cmd.exe or directly
        let (path, args) = if options.command.len() == 1 {
            // Shell command
            (
                "cmd.exe".to_string(),
                vec![
                    "cmd".to_string(),
                    "/d".to_string(),
                    "/c".to_string(),
                    options.command[0].clone(),
                ],
            )
        } else {
            (options.command[0].clone(), options.command.clone())
        };

        // Validate working directory: if empty or nonexistent, fallback to user home or temp dir
        // Matches Go Registry.start:
        // if st, err := os.Stat(dir); dir == "" || err != nil || !st.IsDir() { dir, _ = os.UserHomeDir() }
        let dir = if options.dir.is_empty() || !std::path::Path::new(&options.dir).is_dir() {
            dirs_sys_home().unwrap_or_else(|| std::env::temp_dir().to_string_lossy().to_string())
        } else {
            options.dir.clone()
        };

        let pty_opts = PtyOptions {
            path,
            args,
            dir,
            env: options.env.clone(),
            cols: if options.cols == 0 { 120 } else { options.cols },
            rows: if options.rows == 0 { 36 } else { options.rows },
        };

        let pty = PtyProc::start(pty_opts)
            .map_err(|e| RunnerError::StartFailed(format!("pty start failed: {}", e)))?;

        let pid = pty.pid();
        let history = Arc::new(Mutex::new(options.history.clone()));
        let closed = Arc::new(AtomicBool::new(false));
        let subscribers = Arc::new(Mutex::new(Vec::new()));

        let handle = Arc::new(ProductionHandle {
            id: options.id.clone(),
            pid,
            pty: pty.clone(),
            history: Arc::clone(&history),
            closed: Arc::clone(&closed),
            subscribers: Arc::clone(&subscribers),
        });

        // Background reader thread: captures streaming output into history ring buffer and fans out to subscribers
        let reader_pty = pty.clone();
        let reader_history = Arc::clone(&history);
        let id_for_exit = options.id.clone();
        let notifier_opt = self.on_exit.lock().unwrap().clone();
        let global_out_opt = self.on_output.lock().unwrap().clone();
        let handle_subs = Arc::clone(&subscribers);
        let reader_closed = Arc::clone(&closed);

        thread::Builder::new()
            .name(format!("runner-io-{}", pid))
            .spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader_pty.read_out(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let bytes = &buf[..n];
                            // 1. Maintain scrollback history ring buffer
                            {
                                let mut h = reader_history.lock().unwrap();
                                h.extend_from_slice(bytes);
                                if h.len() > MAX_HISTORY_BYTES {
                                    let excess = h.len() - MAX_HISTORY_BYTES;
                                    h.drain(..excess);
                                }
                            }
                            // 2. Dispatch to live global output subscriber
                            if let Some(ref sub) = global_out_opt {
                                sub(&id_for_exit, bytes);
                            }
                            // 3. Dispatch to handle-specific live subscribers
                            let subs_snapshot = handle_subs.lock().unwrap().clone();
                            for sub in subs_snapshot {
                                sub(&id_for_exit, bytes);
                            }
                        }
                        Err(_) => break,
                    }
                }

                reader_closed.store(true, Ordering::SeqCst);

                // Wait for process exit code
                let mut exit_code = 0;
                let mut count = 0;
                while reader_pty.exit_code().is_none() && count < 100 {
                    thread::sleep(Duration::from_millis(10));
                    count += 1;
                }
                if let Some(code) = reader_pty.exit_code() {
                    exit_code = code;
                }

                if let Some(notifier) = notifier_opt {
                    notifier(&id_for_exit, exit_code);
                }
            })
            .expect("spawn runner io thread");

        self.handles
            .lock()
            .unwrap()
            .insert(options.id, Arc::clone(&handle));

        Ok(handle as Arc<dyn Handle>)
    }

    fn get(&self, id: &str) -> Option<Arc<dyn Handle>> {
        self.handles
            .lock()
            .unwrap()
            .get(id)
            .map(|h| Arc::clone(h) as Arc<dyn Handle>)
    }

    fn remove(&self, id: &str) {
        if let Some(h) = self.handles.lock().unwrap().remove(id) {
            h.closed.store(true, Ordering::SeqCst);
            h.pty.stop(Duration::from_millis(200));
        }
    }
}
