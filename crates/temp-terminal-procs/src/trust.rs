//! Persistent trust store for process definitions.
//!
//! Corresponds to Go `tempterm/internal/procs/trust.go`.
//!
//! Trust stores approved definition variants by project directory in a JSON file
//! on disk (typically `~/.config/tempterm/trust.json` or `~/.config/temp-terminal/trust.json`).
//!
//! The on-disk format is:
//! ```json
//! {
//!   "version": 1,
//!   "projects": {
//!     "/abs/dir": {
//!       "<variant_hash>": "<RFC3339 timestamp>"
//!     }
//!   }
//! }
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use thiserror::Error;

/// Errors that can occur when mutating or reading the trust store.
#[derive(Debug, Error)]
pub enum TrustError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// In-memory and on-disk representation of `trust.json`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
struct TrustFile {
    pub version: i32,
    #[serde(default)]
    pub projects: BTreeMap<String, BTreeMap<String, String>>,
}

/// Stamp recording modification timestamp and size for cache freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TrustStamp {
    modified: Option<SystemTime>,
    size: u64,
}

/// In-memory state protected by internal mutex.
#[derive(Default)]
struct TrustInner {
    stamp: TrustStamp,
    cached: TrustFile,
    have: bool,
}

/// Trust manages user approval of process definition variants per machine.
///
/// Corresponds to Go `tempterm/internal/procs/trust.go:14`.
pub struct Trust {
    path: PathBuf,
    inner: Mutex<TrustInner>,
}

impl Trust {
    /// Opens the trust store kept in the file at `path`.
    ///
    /// The file is read lazily on demand when needed. Does not perform filesystem I/O
    /// and cannot fail.
    ///
    /// Corresponds to Go `OpenTrust(path)` in `tempterm/internal/procs/trust.go:39`.
    pub fn open<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            inner: Mutex::new(TrustInner::default()),
        }
    }

    /// Reports whether the user has trusted `variant` in the project at `dir`.
    ///
    /// An absent, unreadable, or corrupted file returns `false` (trusts nothing).
    ///
    /// Corresponds to Go `(*Trust).Trusted(dir, variant)` in `tempterm/internal/procs/trust.go:46`.
    pub fn is_trusted<P: AsRef<Path>>(&self, dir: P, variant: &str) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let file = match Self::read_locked(&self.path, &mut inner) {
            Ok(f) => f,
            Err(_) => return false,
        };

        let cleaned_dir = clean_path(dir.as_ref().to_str().unwrap_or(""));
        if let Some(variants) = file.projects.get(&cleaned_dir) {
            variants.contains_key(variant)
        } else {
            false
        }
    }

    pub fn add<P: AsRef<Path>>(&self, dir: P, variant: &str) -> Result<(), TrustError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());

        let mut file = match Self::read_locked(&self.path, &mut inner) {
            Ok(f) => f,
            Err(TrustError::Io(e)) if e.kind() == io::ErrorKind::NotFound => TrustFile::default(),
            Err(e) => return Err(e),
        };

        // Invalidate in-memory cache
        inner.have = false;

        file.version = 1;
        let cleaned_dir = clean_path(dir.as_ref().to_str().unwrap_or(""));
        let now_utc = current_rfc3339_utc();

        file.projects
            .entry(cleaned_dir)
            .or_default()
            .insert(variant.to_string(), now_utc);

        let data = serde_json::to_vec_pretty(&file)?;

        if let Some(parent) = self.path.parent() {
            create_dir_all_secure(parent)?;
        }

        write_atomic(&self.path, &data)?;
        Ok(())
    }

    /// Internal read helper holding the mutex lock.
    /// Returns cached state if mtime and size match; otherwise re-reads and parses JSON.
    fn read_locked(path: &Path, inner: &mut TrustInner) -> Result<TrustFile, TrustError> {
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                inner.have = false;
                return Err(TrustError::Io(e));
            }
        };

        let stamp = TrustStamp {
            modified: metadata.modified().ok(),
            size: metadata.len(),
        };

        if inner.have && stamp == inner.stamp {
            return Ok(inner.cached.clone());
        }

        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => return Err(TrustError::Io(e)),
        };

        let parsed: TrustFile = match serde_json::from_slice(&bytes) {
            Ok(f) => f,
            Err(e) => {
                inner.have = false;
                return Err(TrustError::Json(e));
            }
        };

        inner.cached = parsed.clone();
        inner.stamp = stamp;
        inner.have = true;

        Ok(parsed)
    }
}

/// Cleans a path following Go `filepath.Clean` semantics.
///
/// Matches Go:
/// - Empty string -> `"."`
/// - Converts `/` and `\` to system separator
/// - Replaces multiple separators with single separator
/// - Eliminates `.` path components
/// - Resolves inner `..` parent references
/// - Strips trailing separator (unless root)
fn clean_path(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }

    let is_windows = cfg!(windows);
    let normalized = if is_windows {
        p.replace('/', "\\")
    } else {
        p.to_string()
    };

    let sep = if is_windows { '\\' } else { '/' };
    let is_abs = normalized.starts_with(sep)
        || (is_windows && normalized.len() >= 2 && normalized.as_bytes()[1] == b':');

    let mut parts: Vec<&str> = Vec::new();
    let mut prefix = "";
    let mut remainder = normalized.as_str();

    // Extract Windows drive volume if present (e.g., "C:")
    if is_windows && remainder.len() >= 2 && remainder.as_bytes()[1] == b':' {
        prefix = &remainder[..2];
        remainder = &remainder[2..];
    }

    let starts_with_sep = remainder.starts_with(sep);

    for component in remainder.split(sep) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if let Some(last) = parts.last() {
                if *last != ".." {
                    parts.pop();
                    continue;
                }
            }
            if !is_abs {
                parts.push("..");
            }
        } else {
            parts.push(component);
        }
    }

    let mut result = String::with_capacity(p.len());
    result.push_str(prefix);
    if starts_with_sep {
        result.push(sep);
    }

    if parts.is_empty() {
        if result.is_empty() {
            result.push('.');
        }
    } else {
        for (i, part) in parts.iter().enumerate() {
            if i > 0 || (starts_with_sep && !result.ends_with(sep)) {
                result.push(sep);
            }
            result.push_str(part);
        }
    }

    result
}

/// Generates current time as whole-second UTC RFC 3339 string (e.g. `2026-11-04T02:00:00Z`).
///
/// Matches Go `time.Now().UTC().Format(time.RFC3339)` without adding external dependencies.
fn current_rfc3339_utc() -> String {
    let now = SystemTime::now();
    let epoch_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    format_rfc3339_utc(epoch_secs)
}

/// Formats unix epoch seconds into an RFC 3339 UTC string.
fn format_rfc3339_utc(secs: u64) -> String {
    let sec_of_min = (secs % 60) as u32;
    let min_of_hour = ((secs / 60) % 60) as u32;
    let hour_of_day = ((secs / 3600) % 24) as u32;
    let mut days = secs / 86400;

    // Civil day calculation from Unix epoch (1970-01-01)
    let mut year = 1970u32;
    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month = 1u32;
    for &d in &month_days {
        if days >= d {
            days -= d;
            month += 1;
        } else {
            break;
        }
    }
    let day = (days + 1) as u32;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour_of_day, min_of_hour, sec_of_min
    )
}

/// Atomically replaces file at `path` with `data` via a sibling temporary file.
///
/// Matches Go `writeAtomic` in `tempterm/internal/procs/file.go:233`.
fn write_atomic(path: &Path, data: &[u8]) -> Result<(), io::Error> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

    // Sibling temporary file: .${name}.${pid}_${nonce}_${cnt}.tmp
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let pid = std::process::id();
    let cnt = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = dir.join(format!(".{}.{}_{}_{}.tmp", file_name, pid, nonce, cnt));

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
    {
        Ok(f) => f,
        Err(e) => return Err(e),
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }

    let write_res = (|| -> Result<(), io::Error> {
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        atomic_rename(&tmp_path, path)?;
        Ok(())
    })();

    if write_res.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }

    write_res
}

/// Atomic file replacement with Windows sharing violation retry.
///
/// Matches Go `atomicfile.Rename` in `tempterm/internal/atomicfile/rename_windows.go`.
fn atomic_rename(from: &Path, to: &Path) -> Result<(), io::Error> {
    #[cfg(not(windows))]
    {
        fs::rename(from, to)
    }

    #[cfg(windows)]
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let retry_pause = std::time::Duration::from_millis(20);

        loop {
            match fs::rename(from, to) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let is_retryable = is_windows_retryable(&err);
                    if !is_retryable || std::time::Instant::now() >= deadline {
                        return Err(err);
                    }
                    std::thread::sleep(retry_pause);
                }
            }
        }
    }
}

#[cfg(windows)]
fn is_windows_retryable(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }
    if let Some(raw) = err.raw_os_error() {
        // ERROR_ACCESS_DENIED = 5, ERROR_SHARING_VIOLATION = 32
        return raw == 5 || raw == 32;
    }
    false
}

/// Creates directories with 0700 permissions on Unix.
fn create_dir_all_secure(path: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}
