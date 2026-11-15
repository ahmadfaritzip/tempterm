//! Session and Pane Registry managing terminal processes and PTY instances.
//! Corresponds to Go `tempterm/internal/session/registry.go` and `tempterm/internal/session/pane.go`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use temp_terminal_pty::{PtyOptions, PtyProc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneStatus {
    Starting,
    Running,
    Exited,
    Failed,
}

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub id: String,
    pub session: String,
    pub project: String,
    pub role: String,
    pub dir: String,
    pub command: Vec<String>,
    pub status: PaneStatus,
    pub pid: Option<u32>,
    pub since: i64,
}

#[derive(Debug, Clone)]
pub struct StartPaneOptions {
    pub id: String,
    pub session: String,
    pub project: String,
    pub role: String,
    pub dir: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub cols: u16,
    pub rows: u16,
}

impl Default for StartPaneOptions {
    fn default() -> Self {
        Self {
            id: String::new(),
            session: String::new(),
            project: String::new(),
            role: String::new(),
            dir: String::new(),
            command: Vec::new(),
            env: Vec::new(),
            cols: 120,
            rows: 36,
        }
    }
}

pub struct Pane {
    pub info: Mutex<PaneInfo>,
    pub proc: Option<PtyProc>,
    pub history: Mutex<Vec<u8>>,
    pub is_closed: AtomicBool,
}

impl Pane {
    pub fn id(&self) -> String {
        self.info.lock().unwrap().id.clone()
    }

    pub fn write_input(&self, data: &[u8]) -> std::io::Result<usize> {
        if let Some(ref proc) = self.proc {
            proc.write_in(data)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "pane has no active process",
            ))
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), temp_terminal_pty::PtyError> {
        if let Some(ref proc) = self.proc {
            proc.resize(cols, rows)
        } else {
            Ok(())
        }
    }

    pub fn close(&self) {
        self.is_closed.store(true, Ordering::SeqCst);
        if let Some(ref proc) = self.proc {
            proc.stop(Duration::from_millis(300));
        }
    }

    /// Returns the active foreground child process name running under this pane's PTY.
    pub fn active_foreground_process(&self) -> Option<String> {
        self.proc
            .as_ref()
            .and_then(|p| p.active_foreground_process())
    }
}

pub type OutputSubscriber = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;
pub type ExitSubscriber = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Registry {
    panes: Mutex<HashMap<String, Arc<Pane>>>,
    order: Mutex<Vec<String>>,
    starting: Mutex<HashMap<String, bool>>,
    on_output: Arc<Mutex<Option<OutputSubscriber>>>,
    output_subscribers: Arc<Mutex<Vec<OutputSubscriber>>>,
    exit_subscribers: Arc<Mutex<Vec<ExitSubscriber>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("session: pane exists")]
    Exists,
    #[error("pty error: {0}")]
    Pty(#[from] temp_terminal_pty::PtyError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Registry {
    pub fn new() -> Self {
        Self {
            panes: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
            starting: Mutex::new(HashMap::new()),
            on_output: Arc::new(Mutex::new(None)),
            output_subscribers: Arc::new(Mutex::new(Vec::new())),
            exit_subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Sets or replaces the single primary output subscriber callback.
    pub fn set_on_output<F>(&self, cb: F)
    where
        F: Fn(&str, &[u8]) + Send + Sync + 'static,
    {
        *self.on_output.lock().unwrap() = Some(Arc::new(cb));
    }

    /// Adds an additional output subscriber without removing existing subscribers.
    pub fn add_on_output<F>(&self, cb: F)
    where
        F: Fn(&str, &[u8]) + Send + Sync + 'static,
    {
        self.output_subscribers.lock().unwrap().push(Arc::new(cb));
    }

    /// Adds an exit subscriber called when a pane's underlying process exits.
    pub fn add_on_exit<F>(&self, cb: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.exit_subscribers.lock().unwrap().push(Arc::new(cb));
    }

    pub fn start(&self, opts: StartPaneOptions) -> Result<Arc<Pane>, RegistryError> {
        let id = opts.id.clone();
        {
            let mut starting = self.starting.lock().unwrap();
            let panes = self.panes.lock().unwrap();
            if panes.contains_key(&id) || starting.contains_key(&id) {
                return Err(RegistryError::Exists);
            }
            starting.insert(id.clone(), true);
        }

        let (path, args) = if opts.command.is_empty() {
            #[cfg(windows)]
            let p = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
            #[cfg(not(windows))]
            let p = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
            (p, Vec::new())
        } else {
            (opts.command[0].clone(), opts.command[1..].to_vec())
        };

        let pty_opts = PtyOptions {
            path,
            args,
            dir: opts.dir.clone(),
            env: opts.env.clone(),
            cols: if opts.cols == 0 { 120 } else { opts.cols },
            rows: if opts.rows == 0 { 36 } else { opts.rows },
        };

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let spawn_res = PtyProc::start(pty_opts);

        let mut starting = self.starting.lock().unwrap();
        starting.remove(&id);

        let proc = match spawn_res {
            Ok(p) => p,
            Err(e) => return Err(RegistryError::Pty(e)),
        };

        let pid = proc.pid();

        let pane = Arc::new(Pane {
            info: Mutex::new(PaneInfo {
                id: id.clone(),
                session: opts.session,
                project: opts.project,
                role: opts.role,
                dir: opts.dir,
                command: opts.command,
                status: PaneStatus::Running,
                pid: Some(pid),
                since: now_ms,
            }),
            proc: Some(proc.clone()),
            history: Mutex::new(Vec::new()),
            is_closed: AtomicBool::new(false),
        });

        {
            let mut panes = self.panes.lock().unwrap();
            let mut order = self.order.lock().unwrap();
            panes.insert(id.clone(), Arc::clone(&pane));
            order.push(id.clone());
        }

        // Spawn background output reader loop
        let pane_clone = Arc::clone(&pane);
        let on_output_self = Arc::clone(&self.on_output);
        let subs_self = Arc::clone(&self.output_subscribers);
        let exits_self = Arc::clone(&self.exit_subscribers);
        let pane_id = id.clone();

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                let read_res = proc.read_out(&mut buf);
                match read_res {
                    Ok(n) if n > 0 => {
                        let chunk = &buf[..n];
                        {
                            let mut hist = pane_clone.history.lock().unwrap();
                            if hist.len() + n > 2 * 1024 * 1024 {
                                hist.drain(0..n);
                            }
                            hist.extend_from_slice(chunk);
                        }
                        let cb_opt = on_output_self.lock().unwrap().clone();
                        if let Some(ref cb) = cb_opt {
                            cb(&pane_id, chunk);
                        }
                        let subs = subs_self.lock().unwrap().clone();
                        for sub in subs {
                            sub(&pane_id, chunk);
                        }
                    }
                    _ => break,
                }
            }

            {
                let mut info = pane_clone.info.lock().unwrap();
                info.status = PaneStatus::Exited;
            }

            let exits = exits_self.lock().unwrap().clone();
            for exit_cb in exits {
                exit_cb(&pane_id);
            }
        });

        Ok(pane)
    }

    pub fn get(&self, id: &str) -> Option<Arc<Pane>> {
        let panes = self.panes.lock().unwrap();
        panes.get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        let removed = {
            let mut panes = self.panes.lock().unwrap();
            let mut order = self.order.lock().unwrap();
            order.retain(|item| item != id);
            panes.remove(id)
        };

        if let Some(pane) = removed {
            pane.close();
        }
    }

    pub fn list(&self) -> Vec<PaneInfo> {
        let panes = self.panes.lock().unwrap();
        let order = self.order.lock().unwrap();
        let mut out = Vec::with_capacity(order.len());
        for id in order.iter() {
            if let Some(p) = panes.get(id) {
                out.push(p.info.lock().unwrap().clone());
            }
        }
        out
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
