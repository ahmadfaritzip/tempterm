//! Process supervisor and runner contracts for temp-terminal.
//! Corresponds to Go `tempterm/internal/procs/supervisor.go`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::def::Def;
use crate::ready::{
    is_listening, resolve_run_port, DEFAULT_ALIVE_DELAY, DEFAULT_CHECK_PERIOD,
    DEFAULT_PROBE_TIMEOUT,
};
use crate::restart::{
    calculate_backoff, format_manual_restart_banner, format_restart_banner, should_restart,
    update_restart_window, DEFAULT_MAX_RESTARTS, DEFAULT_RESTART_BASE, DEFAULT_RESTART_WINDOW,
};
use crate::trust::Trust;
use crate::watch::{
    make_ignore_filter, match_glob, poll_changes, Debouncer, WatchError, WatchFunc,
    DEFAULT_POLL_PERIOD, DEFAULT_WATCH_DEBOUNCE,
};

/// Errors returned by a [`Runner`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RunnerError {
    #[error("runner start failed: {0}")]
    StartFailed(String),
    #[error("process handle not found: {0}")]
    NotFound(String),
}

/// Options passed to [`Runner::start`] to launch a pane.
/// Corresponds to Go `session.StartOptions` (`tempterm/internal/session/pane.go:73`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartOptions {
    pub id: String,
    pub session: String,
    pub role: String,
    pub dir: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub exec: bool,
    pub history: Vec<u8>,
    pub cols: u16,
    pub rows: u16,
}

/// Handle to a running process pane.
/// Corresponds to Go `session.Handle` (`tempterm/internal/session/host.go:18`).
pub trait Handle: Send + Sync {
    /// Unique pane identifier.
    fn id(&self) -> &str;

    /// Process operating system PID.
    fn pid(&self) -> u32;

    /// Terminal scrollback history captured so far.
    fn history(&self) -> Vec<u8>;

    /// Liveness probe: reports whether the process pane is actively running.
    /// Corresponds to Go `handle.Info().Status != session.Exited`.
    fn is_alive(&self) -> bool;
}

/// Runner starts and ends the panes processes run in.
/// Corresponds to Go `procs.Runner` (`tempterm/internal/procs/supervisor.go:21-25`).
pub trait Runner: Send + Sync {
    /// Start a process according to the given options.
    fn start(&self, options: StartOptions) -> Result<Arc<dyn Handle>, RunnerError>;

    /// Retrieve an existing process handle by ID.
    fn get(&self, id: &str) -> Option<Arc<dyn Handle>>;

    /// Remove and end a process pane by ID.
    fn remove(&self, id: &str);
}

/// Status is a process's state in the supervisor.
/// Corresponds to Go `procs.Status` (`tempterm/internal/procs/supervisor.go:28-38`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessStatus {
    Stopped,
    Starting,
    Running,
    Exited,
    Failed,
    Paused,
    Untrusted,
}

impl ProcessStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Failed => "failed",
            Self::Paused => "paused",
            Self::Untrusted => "untrusted",
        }
    }
}

/// Process lifecycle event kind.
/// Corresponds to Go `procs.EventKind` (`tempterm/internal/procs/supervisor.go:66-73`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Started,
    Stopped,
    Failed,
    Paused,
}

/// Event reports a process lifecycle event.
/// Corresponds to Go `procs.Event` (`tempterm/internal/procs/supervisor.go:76-80`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub project: String,
    pub name: String,
    pub kind: EventKind,
    pub code: i32,
}

/// Process as the UI and CLI see it.
/// Corresponds to Go `procs.Proc` (`tempterm/internal/procs/supervisor.go:41-63`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proc {
    #[serde(flatten)]
    pub def: Def,
    pub status: ProcessStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pane: String,
    pub session: String,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub pid: u32,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub restarts: usize,
    #[serde(default, rename = "lastExit", skip_serializing_if = "is_zero_i32")]
    pub last_exit: i32,
    #[serde(
        default,
        rename = "lastChange",
        skip_serializing_if = "String::is_empty"
    )]
    pub last_change: String,
    pub since: i64,
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changed: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub share: String,
    #[serde(default, rename = "shareVia", skip_serializing_if = "String::is_empty")]
    pub share_via: String,
}

fn is_zero_u32(val: &u32) -> bool {
    *val == 0
}

fn is_zero_usize(val: &usize) -> bool {
    *val == 0
}

fn is_zero_i32(val: &i32) -> bool {
    *val == 0
}

impl Proc {
    /// Return the name of the process from its definition.
    pub fn name(&self) -> &str {
        &self.def.name
    }
}

/// Internal state tracked per process definition.
/// Corresponds to Go `procs.procState` (`tempterm/internal/procs/supervisor.go:121-137`).
struct ProcState {
    proc: Proc,
    def: Def,
    dir: String,
    handle: Option<Arc<dyn Handle>>,
    running: String,
    generation: u64,
    stopped: bool,
    restarts: Vec<u64>,
    cancel_readiness: Option<Arc<AtomicBool>>,
    cancel_watch: Option<Arc<AtomicBool>>,
    port: u16,
}

/// Internal state tracked per project.
/// Corresponds to Go `procs.projectState` (`tempterm/internal/procs/supervisor.go:115-120`).
#[derive(Default)]
struct ProjectState {
    dir: String,
    order: Vec<String>,
    procs: HashMap<String, ProcState>,
    loaded: bool,
}

/// Supervisor errors.
#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("process not found: {0}")]
    NotFound(String),
    #[error("{0} isn't trusted yet: review it in tempterm")]
    Untrusted(String),
    #[error("start failed: {0}")]
    StartFailed(String),
}

/// Returns the stable session ID for a project process.
/// Corresponds to Go `procs.SessionID` (`tempterm/internal/procs/supervisor.go:148`).
pub fn session_id(project_id: &str, name: &str) -> String {
    format!("proc-{}-{}", project_id, name)
}

/// Generates a pane ID for a process run.
/// Corresponds to Go `paneID` (`tempterm/internal/procs/supervisor.go:740-746`).
fn generate_pane_id(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let lower_bytes = (nanos & 0xFFFF_FFFF) as u32;
    format!("proc-{}-{:08x}", name, lower_bytes)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Callback type for supervisor lifecycle events.
pub type EventCallback = Box<dyn Fn(Event) + Send + Sync>;

/// Callback type for supervisor state changes.
pub type ChangeCallback = Box<dyn Fn() + Send + Sync>;

/// Routed is a process with the project it belongs to.
/// Corresponds to Go `procs.Routed` (`tempterm/internal/procs/supervisor.go:248-251`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routed {
    pub project: String,
    pub proc: Proc,
}

/// Supervisor manages project process definitions and their runtime lifecycles.
/// Corresponds to Go `procs.Supervisor` (`tempterm/internal/procs/supervisor.go:83-113`).
pub struct Supervisor {
    runner: Arc<dyn Runner>,
    trust: Mutex<Option<Trust>>,
    projects: Mutex<HashMap<String, ProjectState>>,
    closed: Mutex<bool>,
    restart_window: Duration,
    max_restarts: usize,
    restart_base: Duration,
    alive_delay: Duration,
    check_period: Duration,
    probe_timeout: Duration,
    watch_debounce: Duration,
    poll_period: Duration,
    watch_fn: Mutex<Option<WatchFunc>>,
    on_event: Mutex<Option<EventCallback>>,
    on_change: Mutex<Option<ChangeCallback>>,
    self_ref: Mutex<std::sync::Weak<Supervisor>>,
}

impl Supervisor {
    /// Create a new Supervisor with the given runner and optional trust store.
    pub fn new(runner: Arc<dyn Runner>, trust: Option<Trust>) -> Self {
        Self {
            runner,
            trust: Mutex::new(trust),
            projects: Mutex::new(HashMap::new()),
            closed: Mutex::new(false),
            restart_window: DEFAULT_RESTART_WINDOW,
            max_restarts: DEFAULT_MAX_RESTARTS,
            restart_base: DEFAULT_RESTART_BASE,
            alive_delay: DEFAULT_ALIVE_DELAY,
            check_period: DEFAULT_CHECK_PERIOD,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            watch_debounce: DEFAULT_WATCH_DEBOUNCE,
            poll_period: DEFAULT_POLL_PERIOD,
            watch_fn: Mutex::new(None),
            on_event: Mutex::new(None),
            on_change: Mutex::new(None),
            self_ref: Mutex::new(std::sync::Weak::new()),
        }
    }

    /// Sets the filesystem watcher implementation (e.g. OS watcher or fake test watcher).
    /// Corresponds to Go `s.Watch = ...` in `tempterm/internal/procs/supervisor.go:94`.
    pub fn set_watch_fn(&self, watch_fn: WatchFunc) {
        *self.watch_fn.lock().unwrap() = Some(watch_fn);
    }

    /// Overrides default watch debounce and polling durations (useful for fast tests).
    pub fn with_watch_limits(mut self, debounce: Duration, poll_period: Duration) -> Self {
        self.watch_debounce = debounce;
        self.poll_period = poll_period;
        self
    }

    /// Attach an Arc reference to the supervisor for background async tasks.
    pub fn init(self: &Arc<Self>) {
        *self.self_ref.lock().unwrap() = Arc::downgrade(self);
    }

    /// Configure supervisor parameters (for testability or custom limits).
    pub fn with_limits(
        mut self,
        restart_window: Duration,
        max_restarts: usize,
        restart_base: Duration,
    ) -> Self {
        self.restart_window = restart_window;
        self.max_restarts = max_restarts;
        self.restart_base = restart_base;
        self
    }

    /// Configure readiness parameters (for fast, deterministic tests).
    pub fn with_readiness_limits(
        mut self,
        alive_delay: Duration,
        check_period: Duration,
        probe_timeout: Duration,
    ) -> Self {
        self.alive_delay = alive_delay;
        self.check_period = check_period;
        self.probe_timeout = probe_timeout;
        self
    }

    /// Register a callback for lifecycle events.
    pub fn set_on_event<F>(&self, callback: F)
    where
        F: Fn(Event) + Send + Sync + 'static,
    {
        let mut guard = self.on_event.lock().unwrap();
        *guard = Some(Box::new(callback));
    }

    /// Register a callback for supervisor state changes.
    /// Corresponds to Go `s.OnChange` (`tempterm/internal/procs/supervisor.go:85, 700-703`).
    pub fn set_on_change<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut guard = self.on_change.lock().unwrap();
        *guard = Some(Box::new(callback));
    }

    fn emit(&self, event: Event) {
        let guard = self.on_event.lock().unwrap();
        if let Some(cb) = guard.as_ref() {
            cb(event);
        }
    }

    fn changed(&self) {
        let guard = self.on_change.lock().unwrap();
        if let Some(cb) = guard.as_ref() {
            cb();
        }
    }

    /// Mark the supervisor closed, stopping background activities.
    pub fn close(&self) {
        let mut closed = self.closed.lock().unwrap();
        *closed = true;
        let mut projects = self.projects.lock().unwrap();
        for ps in projects.values_mut() {
            for p in ps.procs.values_mut() {
                if let Some(cancel_r) = p.cancel_readiness.take() {
                    cancel_r.store(true, Ordering::SeqCst);
                }
                if let Some(cancel_w) = p.cancel_watch.take() {
                    cancel_w.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    /// Report whether the supervisor is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap()
    }

    /// Load or reload process definitions for a project.
    /// Corresponds to Go `Supervisor.Load` (`tempterm/internal/procs/supervisor.go:151-224`).
    pub fn load(&self, project_id: &str, dir: &str, defs: Vec<Def>) {
        let autostart_names = {
            let mut projects = self.projects.lock().unwrap();
            let ps = projects.entry(project_id.to_string()).or_default();
            ps.dir = dir.to_string();
            let first = !ps.loaded;
            ps.loaded = true;

            let mut order = Vec::with_capacity(defs.len());
            let mut present = HashMap::new();
            let mut autostart = Vec::new();

            let trust_guard = self.trust.lock().unwrap();

            for d in defs {
                present.insert(d.name.clone(), true);
                order.push(d.name.clone());

                let trusted = trust_guard
                    .as_ref()
                    .map(|t| t.is_trusted(dir, &d.variant()))
                    .unwrap_or(false);

                if let Some(old) = ps.procs.get_mut(&d.name) {
                    if old.handle.is_some() || old.proc.status == ProcessStatus::Starting {
                        old.def = d.clone();
                        old.dir = dir.to_string();
                        old.proc.def = d.clone();
                        old.proc.trusted = trusted;
                        if old.port > 0 && d.port == 0 && !d.url.is_empty() {
                            old.proc.def.port = old.port as i64;
                        }
                        old.proc.changed = old.running != d.variant();
                        if old.proc.changed {
                            old.proc.message = "definition changed; restart to apply".to_string();
                        } else {
                            old.proc.message.clear();
                        }
                    } else {
                        old.def = d.clone();
                        old.dir = dir.to_string();
                        old.proc.def = d.clone();
                        old.proc.trusted = trusted;
                        old.proc.changed = false;
                        old.proc.message.clear();
                        if !trusted {
                            old.proc.status = ProcessStatus::Untrusted;
                        } else if old.proc.status == ProcessStatus::Untrusted {
                            old.proc.status = ProcessStatus::Stopped;
                        }
                    }
                } else {
                    let status = if trusted {
                        ProcessStatus::Stopped
                    } else {
                        ProcessStatus::Untrusted
                    };
                    let proc = Proc {
                        def: d.clone(),
                        status,
                        pane: String::new(),
                        session: session_id(project_id, &d.name),
                        pid: 0,
                        restarts: 0,
                        last_exit: 0,
                        last_change: String::new(),
                        since: now_millis(),
                        trusted,
                        changed: false,
                        message: String::new(),
                        link: String::new(),
                        share: String::new(),
                        share_via: String::new(),
                    };
                    ps.procs.insert(
                        d.name.clone(),
                        ProcState {
                            proc,
                            def: d.clone(),
                            dir: dir.to_string(),
                            handle: None,
                            running: String::new(),
                            generation: 0,
                            stopped: false,
                            restarts: Vec::new(),
                            cancel_readiness: None,
                            cancel_watch: None,
                            port: 0,
                        },
                    );
                }

                if first && trusted && d.autostart {
                    autostart.push(d.name.clone());
                }
            }

            // Clean up removed processes
            let names_to_check: Vec<String> = ps.procs.keys().cloned().collect();
            for name in names_to_check {
                if present.contains_key(&name) {
                    continue;
                }
                if let Some(p) = ps.procs.get_mut(&name) {
                    if p.handle.is_none() {
                        ps.procs.remove(&name);
                        continue;
                    }
                    p.proc.changed = true;
                    p.proc.message = "no longer defined".to_string();
                }
            }

            for name in &ps.order {
                if !present.contains_key(name) {
                    if let Some(p) = ps.procs.get(name) {
                        if p.handle.is_some() {
                            order.push(name.clone());
                        }
                    }
                }
            }

            ps.order = order;
            autostart
        };

        self.changed();

        for name in autostart_names {
            let _ = self.start(project_id, &name);
        }
    }

    /// List all processes defined for a project in definition order.
    /// Corresponds to Go `Supervisor.List` (`tempterm/internal/procs/supervisor.go:227-245`).
    pub fn list(&self, project_id: &str) -> Vec<Proc> {
        let projects = self.projects.lock().unwrap();
        let Some(ps) = projects.get(project_id) else {
            return Vec::new();
        };

        let mut out = Vec::with_capacity(ps.order.len());
        for name in &ps.order {
            if let Some(p) = ps.procs.get(name) {
                out.push(p.proc.clone());
            }
        }
        out
    }

    /// Returns all projects' processes: projects sorted by ID, and processes within each project in definition order.
    /// Corresponds to Go `Supervisor.All` (`tempterm/internal/procs/supervisor.go:253-271`).
    pub fn all(&self) -> Vec<Routed> {
        let projects = self.projects.lock().unwrap();
        let mut ids: Vec<&String> = projects.keys().collect();
        ids.sort();

        let mut out = Vec::new();
        for id in ids {
            if let Some(ps) = projects.get(id) {
                for name in &ps.order {
                    if let Some(p) = ps.procs.get(name) {
                        out.push(Routed {
                            project: id.clone(),
                            proc: p.proc.clone(),
                        });
                    }
                }
            }
        }
        out
    }

    /// Returns processes that currently have or previously had a pane assigned.
    /// Corresponds to Go `Supervisor.Sessions` (`tempterm/internal/procs/supervisor.go:273-283`).
    pub fn sessions(&self, project_id: &str) -> Vec<Proc> {
        self.list(project_id)
            .into_iter()
            .filter(|p| !p.pane.is_empty())
            .collect()
    }

    /// Trust records the current definition's variant as trusted in the trust store and marks the process trusted.
    /// Corresponds to Go `Supervisor.Trust` (`tempterm/internal/procs/supervisor.go:403-430`).
    pub fn trust_process(&self, project_id: &str, name: &str) -> Result<(), SupervisorError> {
        let (dir, variant) = {
            let projects = self.projects.lock().unwrap();
            let ps = projects
                .get(project_id)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;
            let p = ps
                .procs
                .get(name)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;
            (ps.dir.clone(), p.def.variant())
        };

        {
            let mut trust_guard = self.trust.lock().unwrap();
            if let Some(t) = trust_guard.as_mut() {
                t.add(&dir, &variant).map_err(|e| {
                    SupervisorError::StartFailed(format!("trust store error: {}", e))
                })?;
            }
        }

        {
            let mut projects = self.projects.lock().unwrap();
            if let Some(ps) = projects.get_mut(project_id) {
                if let Some(p) = ps.procs.get_mut(name) {
                    p.proc.trusted = true;
                    if p.proc.status == ProcessStatus::Untrusted {
                        p.proc.status = ProcessStatus::Stopped;
                    }
                    p.proc.since = now_millis();
                }
            }
        }

        self.changed();
        Ok(())
    }

    /// Retrieve the current generation counter for a process.
    pub fn process_generation(&self, project_id: &str, name: &str) -> Option<u64> {
        let projects = self.projects.lock().unwrap();
        projects
            .get(project_id)
            .and_then(|ps| ps.procs.get(name))
            .map(|p| p.generation)
    }

    /// Start a process by project ID and name.
    /// Corresponds to Go `Supervisor.Start` / `start` / `startRun`
    /// (`tempterm/internal/procs/supervisor.go:273-333, 465-538`).
    pub fn start(&self, project_id: &str, name: &str) -> Result<(), SupervisorError> {
        self.start_internal(project_id, name, Vec::new())
    }

    /// Manual restart: stops and starts a process without sticky stop, preserving scrollback.
    /// Corresponds to Go `Supervisor.Restart` (`tempterm/internal/procs/supervisor.go:374-400`).
    pub fn restart(&self, project_id: &str, name: &str) -> Result<(), SupervisorError> {
        let history = {
            let mut projects = self.projects.lock().unwrap();
            let ps = projects
                .get_mut(project_id)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;
            let p = ps
                .procs
                .get_mut(name)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;

            if !p.proc.trusted {
                return Err(SupervisorError::Untrusted(name.to_string()));
            }

            let mut hist = Vec::new();
            if let Some(h) = &p.handle {
                hist.extend_from_slice(&h.history());
                hist.extend_from_slice(&format_manual_restart_banner());
            }
            hist
        };

        self.stop(project_id, name)?;

        // Clear sticky stop flag before restarting
        {
            let mut projects = self.projects.lock().unwrap();
            if let Some(ps) = projects.get_mut(project_id) {
                if let Some(p) = ps.procs.get_mut(name) {
                    p.stopped = false;
                }
            }
        }

        self.start_internal(project_id, name, history)
    }

    fn start_internal(
        &self,
        project_id: &str,
        name: &str,
        history: Vec<u8>,
    ) -> Result<(), SupervisorError> {
        let (def, dir, gen, pane_id) = {
            let mut projects = self.projects.lock().unwrap();
            let ps = projects
                .get_mut(project_id)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;
            let p = ps
                .procs
                .get_mut(name)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;

            if !p.proc.trusted {
                return Err(SupervisorError::Untrusted(name.to_string()));
            }

            if p.handle.is_some()
                && p.proc.status != ProcessStatus::Exited
                && p.proc.status != ProcessStatus::Failed
                && p.proc.status != ProcessStatus::Paused
            {
                return Ok(());
            }

            if let Some(cancel) = p.cancel_readiness.take() {
                cancel.store(true, Ordering::SeqCst);
            }
            if let Some(cancel_w) = p.cancel_watch.take() {
                cancel_w.store(true, Ordering::SeqCst);
            }

            p.stopped = false;
            p.restarts.clear();
            p.proc.restarts = 0;
            p.proc.status = ProcessStatus::Starting;
            p.proc.message.clear();
            p.proc.since = now_millis();
            p.generation += 1;

            let gen = p.generation;
            let pane_id = generate_pane_id(name);
            (p.def.clone(), p.dir.clone(), gen, pane_id)
        };

        let port = resolve_run_port(&def);

        // Construct StartOptions
        let mut env_vec = Vec::with_capacity(def.env.len() + 1);
        let mut keys: Vec<&String> = def.env.keys().collect();
        keys.sort();
        for k in keys {
            env_vec.push(format!("{}={}", k, def.env[k]));
        }
        if !def.env.contains_key("PORT") && port > 0 {
            env_vec.push(format!("PORT={}", port));
        }

        let full_dir = if def.dir.is_empty() {
            dir.clone()
        } else {
            let p = Path::new(&dir).join(&def.dir);
            p.to_string_lossy().to_string()
        };

        let opts = StartOptions {
            id: pane_id.clone(),
            session: session_id(project_id, name),
            role: "process".to_string(),
            dir: full_dir,
            command: vec![def.command.clone()],
            env: env_vec,
            exec: true,
            history,
            cols: 120,
            rows: 36,
        };

        match self.runner.start(opts) {
            Ok(handle) => {
                let cancel_flag = Arc::new(AtomicBool::new(false));
                {
                    let mut projects = self.projects.lock().unwrap();
                    if let Some(ps) = projects.get_mut(project_id) {
                        if let Some(p) = ps.procs.get_mut(name) {
                            if p.generation == gen && !p.stopped && !self.is_closed() {
                                p.proc.pid = handle.pid();
                                p.proc.pane = handle.id().to_string();
                                p.port = port;
                                p.proc.def.port = port as i64;
                                p.proc.status = ProcessStatus::Starting;
                                p.proc.since = now_millis();
                                p.proc.changed = false;
                                p.running = def.variant();
                                p.handle = Some(Arc::clone(&handle));
                                p.cancel_readiness = Some(Arc::clone(&cancel_flag));
                            } else {
                                self.runner.remove(handle.id());
                                return Ok(());
                            }
                        }
                    }
                }

                self.changed();
                self.emit(Event {
                    project: project_id.to_string(),
                    name: name.to_string(),
                    kind: EventKind::Started,
                    code: 0,
                });

                // Spawn asynchronous readiness polling task and watch task if a Tokio runtime is available
                if let Ok(handle_runtime) = tokio::runtime::Handle::try_current() {
                    let self_weak = self.self_ref.lock().unwrap().clone();
                    let proj = project_id.to_string();
                    let proc_name = name.to_string();
                    let h = Arc::clone(&handle);
                    let alive_delay = self.alive_delay;
                    let check_period = self.check_period;
                    let probe_timeout = self.probe_timeout;
                    let def_clone = def.clone();
                    let dir_clone = dir.clone();

                    handle_runtime.spawn(async move {
                        if let Some(supervisor) = self_weak.upgrade() {
                            supervisor.spawn_watch(&proj, &proc_name, &def_clone, &dir_clone, gen);

                            supervisor
                                .poll_readiness(
                                    &proj,
                                    &proc_name,
                                    port,
                                    gen,
                                    h,
                                    cancel_flag,
                                    alive_delay,
                                    check_period,
                                    probe_timeout,
                                )
                                .await;
                        }
                    });
                }

                Ok(())
            }
            Err(e) => {
                let err_msg = format!("start failed: {}", e);
                let mut projects = self.projects.lock().unwrap();
                if let Some(ps) = projects.get_mut(project_id) {
                    if let Some(p) = ps.procs.get_mut(name) {
                        if p.generation == gen {
                            p.proc.status = ProcessStatus::Failed;
                            p.proc.message = err_msg.clone();
                            p.proc.pid = 0;
                            p.proc.since = now_millis();
                        }
                    }
                }
                self.changed();
                self.emit(Event {
                    project: project_id.to_string(),
                    name: name.to_string(),
                    kind: EventKind::Failed,
                    code: 0,
                });
                Err(SupervisorError::StartFailed(err_msg))
            }
        }
    }

    /// Stop ends a process and prevents it from restarting automatically (sticky stop).
    /// Corresponds to Go `Supervisor.Stop` (`tempterm/internal/procs/supervisor.go:336-369`).
    pub fn stop(&self, project_id: &str, name: &str) -> Result<(), SupervisorError> {
        let handle_to_remove = {
            let mut projects = self.projects.lock().unwrap();
            let ps = projects
                .get_mut(project_id)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;
            let p = ps
                .procs
                .get_mut(name)
                .ok_or_else(|| SupervisorError::NotFound(name.to_string()))?;

            if let Some(cancel) = p.cancel_readiness.take() {
                cancel.store(true, Ordering::SeqCst);
            }
            if let Some(cancel_w) = p.cancel_watch.take() {
                cancel_w.store(true, Ordering::SeqCst);
            }

            p.stopped = true;
            let handle = p.handle.take();

            p.proc.status = if p.proc.trusted {
                ProcessStatus::Stopped
            } else {
                ProcessStatus::Untrusted
            };
            p.proc.pid = 0;
            p.proc.changed = false;
            p.proc.message.clear();
            p.proc.since = now_millis();

            handle
        };

        if let Some(h) = handle_to_remove {
            self.runner.remove(h.id());
        }

        self.emit(Event {
            project: project_id.to_string(),
            name: name.to_string(),
            kind: EventKind::Stopped,
            code: 0,
        });
        self.changed();

        Ok(())
    }

    /// Handle a process exit notification from a handle.
    /// Evaluates restart policy, sliding window rate limit, and exponential backoff.
    /// Corresponds to Go `Supervisor.watch` (`tempterm/internal/procs/supervisor.go:535-625`).
    pub async fn handle_exit_async(&self, project_id: &str, name: &str, gen: u64, code: i32) {
        let now_ms = now_millis() as u64;

        enum ExitDecision {
            NoRestart {
                handle: Option<Arc<dyn Handle>>,
            },
            Paused {
                handle: Option<Arc<dyn Handle>>,
            },
            Scheduled {
                handle: Option<Arc<dyn Handle>>,
                delay: Duration,
                history: Vec<u8>,
            },
        }

        let decision = {
            let mut projects = self.projects.lock().unwrap();
            let Some(ps) = projects.get_mut(project_id) else {
                return;
            };
            let Some(p) = ps.procs.get_mut(name) else {
                return;
            };

            // Discard stale or stopped exits
            if p.generation != gen || p.stopped {
                return;
            }

            if let Some(cancel) = p.cancel_readiness.take() {
                cancel.store(true, Ordering::SeqCst);
            }
            if let Some(cancel_w) = p.cancel_watch.take() {
                cancel_w.store(true, Ordering::SeqCst);
            }

            let handle = p.handle.take();
            let prev_history = handle.as_ref().map(|h| h.history()).unwrap_or_default();
            let mut combined_history = prev_history;
            combined_history.extend_from_slice(&format_restart_banner(code));

            p.proc.last_exit = code;
            p.proc.pid = 0;
            p.proc.changed = false;

            if !should_restart(&p.def.restart, code) {
                p.proc.since = now_millis();
                if code == 0 {
                    p.proc.status = ProcessStatus::Exited;
                    p.proc.message.clear();
                } else {
                    p.proc.status = ProcessStatus::Failed;
                    p.proc.message = format!("exited (code {})", code);
                }
                ExitDecision::NoRestart { handle }
            } else {
                p.restarts = update_restart_window(&p.restarts, now_ms, self.restart_window);
                let count = p.restarts.len();
                p.proc.restarts = count;

                if count >= self.max_restarts {
                    let message = format!(
                        "restart paused after {} restarts in a minute",
                        self.max_restarts
                    );
                    p.proc.status = ProcessStatus::Paused;
                    p.proc.since = now_millis();
                    p.proc.message = message;
                    ExitDecision::Paused { handle }
                } else {
                    p.proc.status = ProcessStatus::Starting;
                    p.proc.since = now_millis();
                    let delay = calculate_backoff(count, self.restart_base);
                    ExitDecision::Scheduled {
                        handle,
                        delay,
                        history: combined_history,
                    }
                }
            }
        };

        match decision {
            ExitDecision::NoRestart { handle } => {
                if let Some(h) = handle {
                    self.runner.remove(h.id());
                }
                self.changed();
                if code != 0 {
                    self.emit(Event {
                        project: project_id.to_string(),
                        name: name.to_string(),
                        kind: EventKind::Failed,
                        code,
                    });
                }
            }
            ExitDecision::Paused { handle } => {
                if let Some(h) = handle {
                    self.runner.remove(h.id());
                }
                self.changed();
                self.emit(Event {
                    project: project_id.to_string(),
                    name: name.to_string(),
                    kind: EventKind::Paused,
                    code,
                });
            }
            ExitDecision::Scheduled {
                handle,
                delay,
                history,
            } => {
                if let Some(h) = handle {
                    self.runner.remove(h.id());
                }
                self.changed();

                if delay > Duration::ZERO {
                    tokio::time::sleep(delay).await;
                }

                // Re-validate state after sleep under lock
                let restart_info = {
                    let mut projects = self.projects.lock().unwrap();
                    let Some(ps) = projects.get_mut(project_id) else {
                        return;
                    };
                    let Some(p) = ps.procs.get_mut(name) else {
                        return;
                    };

                    if p.generation != gen || p.stopped || self.is_closed() {
                        return;
                    }

                    if !p.proc.trusted {
                        p.proc.status = ProcessStatus::Untrusted;
                        p.proc.since = now_millis();
                        p.proc.message = "the changed definition isn't trusted yet".to_string();
                        drop(projects);
                        self.changed();
                        return;
                    }

                    p.generation += 1;
                    let new_gen = p.generation;
                    let pane_id = generate_pane_id(name);
                    (p.def.clone(), p.dir.clone(), new_gen, pane_id)
                };

                let (def, dir, new_gen, pane_id) = restart_info;

                let mut env_vec = Vec::with_capacity(def.env.len());
                let mut keys: Vec<&String> = def.env.keys().collect();
                keys.sort();
                for k in keys {
                    env_vec.push(format!("{}={}", k, def.env[k]));
                }

                let full_dir = if def.dir.is_empty() {
                    dir.clone()
                } else {
                    let p = Path::new(&dir).join(&def.dir);
                    p.to_string_lossy().to_string()
                };

                let port = resolve_run_port(&def);
                if !def.env.contains_key("PORT") && port > 0 {
                    env_vec.push(format!("PORT={}", port));
                }

                let opts = StartOptions {
                    id: pane_id.clone(),
                    session: session_id(project_id, name),
                    role: "process".to_string(),
                    dir: full_dir,
                    command: vec![def.command.clone()],
                    env: env_vec,
                    exec: true,
                    history,
                    cols: 120,
                    rows: 36,
                };

                match self.runner.start(opts) {
                    Ok(handle) => {
                        let cancel_flag = Arc::new(AtomicBool::new(false));
                        {
                            let mut projects = self.projects.lock().unwrap();
                            if let Some(ps) = projects.get_mut(project_id) {
                                if let Some(p) = ps.procs.get_mut(name) {
                                    if p.generation == new_gen && !p.stopped && !self.is_closed() {
                                        p.proc.pid = handle.pid();
                                        p.proc.pane = handle.id().to_string();
                                        p.port = port;
                                        p.proc.def.port = port as i64;
                                        p.proc.status = ProcessStatus::Starting;
                                        p.proc.since = now_millis();
                                        p.proc.changed = false;
                                        p.running = def.variant();
                                        p.handle = Some(Arc::clone(&handle));
                                        p.cancel_readiness = Some(Arc::clone(&cancel_flag));
                                    } else {
                                        self.runner.remove(handle.id());
                                        return;
                                    }
                                }
                            }
                        }

                        self.changed();
                        self.emit(Event {
                            project: project_id.to_string(),
                            name: name.to_string(),
                            kind: EventKind::Started,
                            code: 0,
                        });

                        if let Ok(handle_runtime) = tokio::runtime::Handle::try_current() {
                            let self_weak = self.self_ref.lock().unwrap().clone();
                            let proj = project_id.to_string();
                            let proc_name = name.to_string();
                            let h = Arc::clone(&handle);
                            let alive_delay = self.alive_delay;
                            let check_period = self.check_period;
                            let probe_timeout = self.probe_timeout;
                            let def_clone = def.clone();
                            let dir_clone = dir.clone();

                            handle_runtime.spawn(async move {
                                if let Some(supervisor) = self_weak.upgrade() {
                                    supervisor.spawn_watch(
                                        &proj, &proc_name, &def_clone, &dir_clone, new_gen,
                                    );

                                    supervisor
                                        .poll_readiness(
                                            &proj,
                                            &proc_name,
                                            port,
                                            new_gen,
                                            h,
                                            cancel_flag,
                                            alive_delay,
                                            check_period,
                                            probe_timeout,
                                        )
                                        .await;
                                }
                            });
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("start failed: {}", e);
                        let mut projects = self.projects.lock().unwrap();
                        if let Some(ps) = projects.get_mut(project_id) {
                            if let Some(p) = ps.procs.get_mut(name) {
                                if p.generation == new_gen {
                                    p.proc.status = ProcessStatus::Failed;
                                    p.proc.message = err_msg;
                                    p.proc.pid = 0;
                                    p.proc.since = now_millis();
                                }
                            }
                        }
                        self.changed();
                        self.emit(Event {
                            project: project_id.to_string(),
                            name: name.to_string(),
                            kind: EventKind::Failed,
                            code: 0,
                        });
                    }
                }
            }
        }
    }

    /// Synchronous convenience entry for non-restarting exits and test compatibility.
    /// Note: for non-restarting definitions, this updates state immediately.
    pub fn handle_exit(&self, project_id: &str, name: &str, gen: u64, code: i32) {
        let handle_to_remove = {
            let mut projects = self.projects.lock().unwrap();
            let Some(ps) = projects.get_mut(project_id) else {
                return;
            };
            let Some(p) = ps.procs.get_mut(name) else {
                return;
            };

            // Discard stale or stopped exits
            if p.generation != gen || p.stopped {
                return;
            }

            if let Some(cancel) = p.cancel_readiness.take() {
                cancel.store(true, Ordering::SeqCst);
            }
            if let Some(cancel_w) = p.cancel_watch.take() {
                cancel_w.store(true, Ordering::SeqCst);
            }

            let handle = p.handle.take();
            p.proc.last_exit = code;
            p.proc.pid = 0;
            p.proc.changed = false;
            p.proc.since = now_millis();

            if code == 0 {
                p.proc.status = ProcessStatus::Exited;
                p.proc.message.clear();
            } else {
                p.proc.status = ProcessStatus::Failed;
                p.proc.message = format!("exited (code {})", code);
            }

            handle
        };

        if let Some(h) = handle_to_remove {
            self.runner.remove(h.id());
        }

        self.changed();
        if code != 0 {
            self.emit(Event {
                project: project_id.to_string(),
                name: name.to_string(),
                kind: EventKind::Failed,
                code,
            });
        }
    }

    /// Asynchronous readiness polling loop.
    /// Corresponds to Go `Supervisor.ready` (`tempterm/internal/procs/supervisor.go:637-662`).
    #[allow(clippy::too_many_arguments)]
    async fn poll_readiness(
        self: Arc<Self>,
        project_id: &str,
        name: &str,
        port: u16,
        gen: u64,
        handle: Arc<dyn Handle>,
        cancel: Arc<AtomicBool>,
        alive_delay: Duration,
        check_period: Duration,
        probe_timeout: Duration,
    ) {
        let mut interval = tokio::time::interval(check_period);
        let deadline = tokio::time::sleep(alive_delay);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                biased;

                _ = &mut deadline => {
                    self.set_running(project_id, name, gen, handle.id());
                    return;
                }
                _ = interval.tick() => {
                    if cancel.load(Ordering::SeqCst) || self.is_closed() || !handle.is_alive() {
                        return;
                    }

                    // Check if process has exited in supervisor state
                    let is_active = {
                        let projects = self.projects.lock().unwrap();
                        projects
                            .get(project_id)
                            .and_then(|ps| ps.procs.get(name))
                            .map(|p| p.generation == gen && !p.stopped && p.handle.as_ref().map(|h| h.id()) == Some(handle.id()) && p.proc.status == ProcessStatus::Starting)
                            .unwrap_or(false)
                    };

                    if !is_active {
                        return;
                    }

                    if port > 0 && is_listening(port, probe_timeout).await {
                        self.set_running(project_id, name, gen, handle.id());
                        return;
                    }
                }
            }
        }
    }

    /// Transitions a starting process to Running.
    /// Corresponds to Go `Supervisor.setRunning` (`tempterm/internal/procs/supervisor.go:678-688`).
    pub fn set_running(&self, project_id: &str, name: &str, gen: u64, handle_id: &str) {
        let mut projects = self.projects.lock().unwrap();
        let Some(ps) = projects.get_mut(project_id) else {
            return;
        };
        let Some(p) = ps.procs.get_mut(name) else {
            return;
        };

        if p.generation != gen
            || p.stopped
            || self.is_closed()
            || p.handle.as_ref().map(|h| h.id()) != Some(handle_id)
            || p.proc.status != ProcessStatus::Starting
        {
            return;
        }

        if let Some(h) = &p.handle {
            if !h.is_alive() {
                return;
            }
        }

        p.proc.status = ProcessStatus::Running;
        p.proc.since = now_millis();
        p.cancel_readiness = None;
        drop(projects);
        self.changed();
    }

    /// Spawns a filesystem watch for a running process definition if watch globs are present.
    /// Corresponds to Go `Supervisor.watching` (`tempterm/internal/procs/watch.go:30-48`).
    pub fn spawn_watch(
        self: &Arc<Self>,
        project_id: &str,
        name: &str,
        def: &Def,
        root: &str,
        gen: u64,
    ) {
        if def.watch.is_empty() {
            return;
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut projects = self.projects.lock().unwrap();
            let Some(ps) = projects.get_mut(project_id) else {
                return;
            };
            let Some(p) = ps.procs.get_mut(name) else {
                return;
            };

            if p.generation != gen || p.stopped || self.is_closed() {
                return;
            }

            p.cancel_watch = Some(Arc::clone(&cancel_flag));
        }

        let watch_fn = self.watch_fn.lock().unwrap().clone();
        let self_weak = Arc::downgrade(self);
        let project = project_id.to_string();
        let proc_name = name.to_string();
        let globs = def.watch.clone();
        let root_path = Path::new(root).to_path_buf();
        let debounce_delay = self.watch_debounce;
        let poll_period = self.poll_period;

        if let Ok(handle_runtime) = tokio::runtime::Handle::try_current() {
            handle_runtime.spawn(async move {
                Self::run_watch_loop(
                    self_weak,
                    project,
                    proc_name,
                    globs,
                    root_path,
                    gen,
                    cancel_flag,
                    debounce_delay,
                    poll_period,
                    watch_fn,
                )
                .await;
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_watch_loop(
        supervisor_weak: std::sync::Weak<Supervisor>,
        project: String,
        name: String,
        globs: Vec<String>,
        root: PathBuf,
        gen: u64,
        cancel: Arc<AtomicBool>,
        debounce_delay: Duration,
        poll_period: Duration,
        watch_fn: Option<WatchFunc>,
    ) {
        let debouncer = Arc::new(Debouncer::new(debounce_delay));

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<String>>(32);

        let (watch_done_tx, mut watch_done_rx) =
            tokio::sync::oneshot::channel::<Result<(), WatchError>>();

        if let Some(w_fn) = watch_fn {
            let tx_clone = tx.clone();
            let cancel_clone = Arc::clone(&cancel);
            let root_clone = root.clone();

            tokio::task::spawn_blocking(move || {
                let res = w_fn(
                    &root_clone,
                    cancel_clone,
                    Box::new(move |paths| {
                        let _ = tx_clone.blocking_send(paths);
                    }),
                );
                let _ = watch_done_tx.send(res);
            });
        } else {
            let _ = watch_done_tx.send(Err(WatchError::Unsupported(
                "no watch on this system".to_string(),
            )));
        }

        // Process incoming events with debounce timer while concurrently monitoring watcher error
        let mut interval = tokio::time::interval(Duration::from_millis(15));
        let mut polling_started = false;

        loop {
            tokio::select! {
                biased;

                _ = interval.tick() => {
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Some(batch) = debouncer.take_if_ready() {
                        if !batch.is_empty() {
                            if let Some(sup) = supervisor_weak.upgrade() {
                                sup.watch_restart(&project, &name, gen, &batch[0]);
                            }
                            return;
                        }
                    }
                }
                res = &mut watch_done_rx, if !polling_started => {
                    polling_started = true;
                    match res {
                        Ok(Err(e)) => {
                            let err_msg = format!("watch failed: {}", e);
                            if let Some(sup) = supervisor_weak.upgrade() {
                                sup.set_watch_message(&project, &name, gen, &err_msg);
                            }

                            // Start polling fallback
                            let tx_poll = tx.clone();
                            let cancel_poll = Arc::clone(&cancel);
                            let globs_poll = globs.clone();
                            let root_poll = root.clone();
                            let ignore_filter = Arc::from(make_ignore_filter(&root_poll));

                            tokio::spawn(async move {
                                poll_changes(
                                    root_poll,
                                    globs_poll,
                                    ignore_filter,
                                    poll_period,
                                    cancel_poll,
                                    move |hits| {
                                        let _ = tx_poll.try_send(hits);
                                    },
                                )
                                .await;
                            });
                        }
                        _ => {
                            if cancel.load(Ordering::SeqCst) {
                                return;
                            }
                        }
                    }
                }
                Some(paths) = rx.recv() => {
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    // Filter matching globs
                    let mut hits: Vec<String> = Vec::new();
                    for path in paths {
                        if globs.iter().any(|g| match_glob(g, &path)) {
                            hits.push(path);
                        }
                    }
                    if !hits.is_empty() {
                        debouncer.push(&hits);
                    }
                }
            }
        }
    }

    /// Sets an error message on the process if watching failed.
    /// Corresponds to Go `Supervisor.setWatchMessage` (`tempterm/internal/procs/watch.go:272-281`).
    pub fn set_watch_message(&self, project: &str, name: &str, gen: u64, message: &str) {
        let mut projects = self.projects.lock().unwrap();
        if let Some(ps) = projects.get_mut(project) {
            if let Some(p) = ps.procs.get_mut(name) {
                if p.generation == gen {
                    p.proc.message = message.to_string();
                }
            }
        }
        drop(projects);
        self.changed();
    }

    /// Triggers a watch-initiated restart for a modified file.
    /// Corresponds to Go `Supervisor.watchRestart` (`tempterm/internal/procs/watch.go:212-270`).
    pub fn watch_restart(self: &Arc<Self>, project: &str, name: &str, gen: u64, path: &str) {
        if self.is_closed() {
            return;
        }

        let (_def, _dir, history, prev_handle_id) = {
            let mut projects = self.projects.lock().unwrap();
            let Some(ps) = projects.get_mut(project) else {
                return;
            };
            let Some(p) = ps.procs.get_mut(name) else {
                return;
            };

            if p.generation != gen || p.stopped || self.is_closed() || p.handle.is_none() {
                return;
            }

            if !p.proc.trusted {
                p.proc.message = "definition changed; trust it in tempterm to apply".to_string();
                drop(projects);
                self.changed();
                return;
            }

            if let Some(cancel_r) = p.cancel_readiness.take() {
                cancel_r.store(true, Ordering::SeqCst);
            }
            if let Some(cancel_w) = p.cancel_watch.take() {
                cancel_w.store(true, Ordering::SeqCst);
            }

            p.proc.last_change = path.to_string();
            p.proc.message.clear();

            let h = p.handle.take().unwrap();
            let mut hist = h.history();
            hist.extend_from_slice(
                format!("\r\n\x1b[2m── restarted (changed: {}) ──\x1b[0m\r\n", path).as_bytes(),
            );

            p.stopped = false;
            p.proc.status = ProcessStatus::Starting;
            p.proc.since = now_millis();

            (p.def.clone(), p.dir.clone(), hist, h.id().to_string())
        };

        self.runner.remove(&prev_handle_id);
        self.changed();

        // Start replacement run with same definition & new generation
        let _ = self.start_internal(project, name, history);
    }
}
