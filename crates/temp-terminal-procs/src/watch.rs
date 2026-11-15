//! Filesystem watching and path matching for process auto-reloading.
//! Corresponds to Go `tempterm/internal/procs/watch.go` and `tempterm/service/discover.go`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Default debounce delay for file change bursts (300 ms).
/// Corresponds to Go `procs.watchDebounce = 300 * time.Millisecond`.
pub const DEFAULT_WATCH_DEBOUNCE: Duration = Duration::from_millis(300);

/// Default poll period for polling fallback when OS watch is unavailable (1 s).
/// Corresponds to Go `procs.pollPeriod = time.Second`.
pub const DEFAULT_POLL_PERIOD: Duration = Duration::from_secs(1);

/// Maximum file entries walked during polling fallback.
/// Corresponds to Go `procs.pollLimit = 5000`.
pub const POLL_LIMIT: usize = 5000;

/// Default directories skipped during discovery, indexing, and polling.
/// Corresponds to Go `skipDirs` in `tempterm/service/discover.go:42-52`.
pub const DEFAULT_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    ".mygo",
    "dist",
    ".cache",
    ".next",
    "target",
];

/// MatchGlob matches a gitignore-style pattern against a path segment-wise:
/// `*` does not cross `/`, `**` crosses `/`.
/// Corresponds to Go `service.MatchGlob` / `matchGitGlob` in `tempterm/service/discover.go:275-325`.
pub fn match_glob(pattern: &str, path: &str) -> bool {
    let pat = normalize_slashes(pattern);
    let target = normalize_slashes(path);

    let psegs: Vec<&str> = pat.split('/').collect();
    let ssegs: Vec<&str> = target.split('/').collect();
    match_segs(&psegs, &ssegs)
}

fn normalize_slashes(s: &str) -> String {
    let s = s.replace('\\', "/");
    s.trim_start_matches("./").to_string()
}

fn match_segs(pat: &[&str], segs: &[&str]) -> bool {
    if pat.is_empty() {
        return segs.is_empty();
    }
    if pat[0] == "**" {
        for i in 0..=segs.len() {
            if match_segs(&pat[1..], &segs[i..]) {
                return true;
            }
        }
        return false;
    }
    if segs.is_empty() {
        return false;
    }
    if !match_single_glob(pat[0], segs[0]) {
        return false;
    }
    match_segs(&pat[1..], &segs[1..])
}

fn match_single_glob(pat: &str, s: &str) -> bool {
    let pat_bytes = pat.as_bytes();
    let s_bytes = s.as_bytes();
    let mut px = 0;
    let mut sx = 0;
    let mut next_px = usize::MAX;
    let mut next_sx = 0;

    while px < pat_bytes.len() || sx < s_bytes.len() {
        if px < pat_bytes.len() {
            match pat_bytes[px] {
                b'*' => {
                    next_px = px;
                    next_sx = sx + 1;
                    px += 1;
                    continue;
                }
                b'?' => {
                    if sx < s_bytes.len() {
                        px += 1;
                        sx += 1;
                        continue;
                    }
                }
                c => {
                    if sx < s_bytes.len() && c == s_bytes[sx] {
                        px += 1;
                        sx += 1;
                        continue;
                    }
                }
            }
        }
        if next_sx > 0 && next_sx <= s_bytes.len() {
            px = next_px;
            sx = next_sx;
            continue;
        }
        return false;
    }
    true
}

/// Minimal .gitignore representation matching Go `loadGitIgnore` / `gitIgnore` in `tempterm/service/discover.go:200-270`.
#[derive(Debug, Clone, Default)]
pub struct GitIgnore {
    patterns: Vec<GitIgnorePattern>,
}

#[derive(Debug, Clone)]
struct GitIgnorePattern {
    pat: String,
    dir_only: bool,
    anchored: bool,
}

impl GitIgnore {
    pub fn parse(content: &str) -> Self {
        let mut patterns = Vec::new();
        for raw_line in content.lines() {
            let line = raw_line.trim_end_matches('\r').trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                continue;
            }
            let mut pat = line.to_string();
            let mut dir_only = false;
            let mut anchored = false;

            if pat.ends_with('/') {
                dir_only = true;
                pat.pop();
            }
            if pat.starts_with('/') {
                anchored = true;
                pat.remove(0);
            }

            patterns.push(GitIgnorePattern {
                pat,
                dir_only,
                anchored,
            });
        }
        Self { patterns }
    }

    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let ignore_file = dir.join(".gitignore");
        if let Ok(content) = fs::read_to_string(ignore_file) {
            Some(Self::parse(&content))
        } else {
            None
        }
    }

    /// Tests whether a relative path (with forward slashes) is ignored.
    pub fn matches(&self, rel: &str, is_dir: bool) -> bool {
        let rel_clean = normalize_slashes(rel);
        if rel_clean.is_empty() || rel_clean == "." {
            return false;
        }

        let parts: Vec<&str> = rel_clean.split('/').collect();

        for p in &self.patterns {
            if p.dir_only && !is_dir {
                // If the pattern is dir-only (e.g. dist/), but this is a file inside that directory
                // (e.g. dist/x.js), check if any directory segment prefix matches p.pat
                let psegs: Vec<&str> = p.pat.split('/').collect();
                if psegs.len() < parts.len() {
                    let mut matched = false;
                    for start in 0..=(parts.len() - 1 - psegs.len()) {
                        let mut ok = true;
                        for (j, ps) in psegs.iter().enumerate() {
                            if !match_single_glob(ps, parts[start + j]) {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        return true;
                    }
                }
                continue;
            }
            if p.anchored {
                if match_glob(&p.pat, &rel_clean) {
                    return true;
                }
                continue;
            }

            // Unanchored: match against the full path or any trailing component
            if match_glob(&p.pat, &rel_clean) {
                return true;
            }
            for i in 1..parts.len() {
                let sub = parts[i..].join("/");
                if match_glob(&p.pat, &sub) {
                    return true;
                }
            }

            // A dir-only pattern also matches when it names any leading sub-path
            if p.dir_only {
                let psegs: Vec<&str> = p.pat.split('/').collect();
                if psegs.len() <= parts.len() {
                    for start in 0..=(parts.len() - psegs.len()) {
                        let mut ok = true;
                        for (j, ps) in psegs.iter().enumerate() {
                            if !match_single_glob(ps, parts[start + j]) {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }
}

/// Filter function deciding whether a given relative path (and directory status) should be ignored.
pub type IgnoreFilter = Box<dyn Fn(&str, bool) -> bool + Send + Sync>;

/// Shared ignore filter for asynchronous tasks.
pub type SharedIgnoreFilter = Arc<dyn Fn(&str, bool) -> bool + Send + Sync>;

/// Evaluates default skip directories and .gitignore rules for a root directory.
/// Corresponds to Go `service.IgnoreFunc` (`tempterm/service/discover.go:171-198`).
pub fn make_ignore_filter(root: &Path) -> IgnoreFilter {
    let gitignore = GitIgnore::load_from_dir(root);
    Box::new(move |rel_path: &str, is_dir: bool| {
        let rel = normalize_slashes(rel_path);
        if rel.is_empty() || rel == "." {
            return false;
        }

        for s in rel.split('/') {
            if s == ".git" || DEFAULT_SKIP_DIRS.contains(&s) {
                return true;
            }
        }

        if let Some(ref ig) = gitignore {
            ig.matches(&rel, is_dir)
        } else {
            false
        }
    })
}

/// Stamp records modification time and size of a file.
/// Corresponds to Go `procs.stamp` (`tempterm/internal/procs/watch.go:283-294`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileStamp {
    pub size: u64,
    pub modified_nanos: u128,
}

impl FileStamp {
    pub fn of(path: &Path) -> Self {
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len();
            let modified_nanos = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            Self {
                size,
                modified_nanos,
            }
        } else {
            Self::default()
        }
    }
}

/// Walks `root` collecting relative paths that match `globs`, skipping ignored files, up to `POLL_LIMIT`.
/// Corresponds to Go `Supervisor.pollFiles` (`tempterm/internal/procs/watch.go:167-208`).
pub fn poll_files(
    root: &Path,
    globs: &[String],
    ignore: &(dyn Fn(&str, bool) -> bool + Send + Sync),
) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let rel = match path.strip_prefix(root) {
                Ok(r) => normalize_slashes(&r.to_string_lossy()),
                Err(_) => continue,
            };

            if rel.is_empty() || rel == "." {
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                if ignore(&rel, true) {
                    continue;
                }
                stack.push(path);
            } else {
                if ignore(&rel, false) {
                    continue;
                }
                if globs.iter().any(|g| match_glob(g, &rel)) {
                    out.push(rel);
                    if out.len() >= POLL_LIMIT {
                        return out;
                    }
                }
            }
        }
    }

    out.sort();
    out
}

/// Abstract folder watch function signature.
/// Corresponds to Go `procs.WatchFunc` (`tempterm/internal/procs/watch.go:14`).
pub type WatchFunc = Arc<
    dyn Fn(
            &Path,
            Arc<AtomicBool>,
            Box<dyn Fn(Vec<String>) + Send + Sync + 'static>,
        ) -> Result<(), WatchError>
        + Send
        + Sync,
>;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("watch failed: {0}")]
    Failed(String),
    #[error("watch not supported on this platform: {0}")]
    Unsupported(String),
}

/// Polls files matching `globs` periodically as a fallback when an OS watcher fails.
/// Corresponds to Go `Supervisor.pollChanges` (`tempterm/internal/procs/watch.go:106-146`).
pub async fn poll_changes<F>(
    root: PathBuf,
    globs: Vec<String>,
    ignore: SharedIgnoreFilter,
    period: Duration,
    cancel: Arc<AtomicBool>,
    on_change: F,
) where
    F: Fn(Vec<String>) + Send + Sync + 'static,
{
    let mut seen: HashMap<String, FileStamp> = HashMap::new();
    let mut first = true;

    loop {
        if cancel.load(Ordering::SeqCst) {
            break;
        }

        let mut now: HashMap<String, FileStamp> = HashMap::new();
        let matched = poll_files(&root, &globs, ignore.as_ref());

        for path in matched {
            let full_path = root.join(&path);
            now.insert(path, FileStamp::of(&full_path));
        }

        if !first {
            let mut hits = Vec::new();
            for (path, st) in &now {
                match seen.get(path) {
                    Some(old) if old == st => {}
                    _ => hits.push(path.clone()),
                }
            }
            hits.sort();
            if !hits.is_empty() {
                on_change(hits);
            }
        }

        seen = now;
        first = false;

        tokio::time::sleep(period).await;
    }
}

/// Helper that manages event debouncing and coalescing for file changes.
/// Matches Go `watchChanges` debounce timer logic (`tempterm/internal/procs/watch.go:50-98`).
pub struct Debouncer {
    delay: Duration,
    pending: Mutex<Vec<String>>,
    fire_at: Mutex<Option<Instant>>,
}

impl Debouncer {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: Mutex::new(Vec::new()),
            fire_at: Mutex::new(None),
        }
    }

    /// Adds paths to pending batch and returns true if this change scheduled a new timer.
    pub fn push(&self, paths: &[String]) -> bool {
        let mut p = self.pending.lock().unwrap();
        p.extend_from_slice(paths);

        let mut f = self.fire_at.lock().unwrap();
        if f.is_none() {
            *f = Some(Instant::now() + self.delay);
            true
        } else {
            false
        }
    }

    /// Takes the accumulated batch if the debounce deadline has arrived.
    pub fn take_if_ready(&self) -> Option<Vec<String>> {
        let mut f = self.fire_at.lock().unwrap();
        if let Some(target) = *f {
            if Instant::now() >= target {
                *f = None;
                let mut p = self.pending.lock().unwrap();
                let batch = std::mem::take(&mut *p);
                if !batch.is_empty() {
                    return Some(batch);
                }
            }
        }
        None
    }

    pub fn reset(&self) {
        *self.pending.lock().unwrap() = Vec::new();
        *self.fire_at.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_glob_segments() {
        assert!(match_glob("src/**", "src/main.rs"));
        assert!(match_glob("src/**", "src/nested/sub/file.rs"));
        assert!(!match_glob("src/**", "README.md"));
        assert!(match_glob("*.txt", "hello.txt"));
        assert!(!match_glob("*.txt", "sub/hello.txt"));
        assert!(match_glob("**/*.txt", "sub/hello.txt"));
        assert!(match_glob("src/*.rs", "src/lib.rs"));
        assert!(!match_glob("src/*.rs", "src/sub/lib.rs"));
    }

    #[test]
    fn test_gitignore_rules() {
        let content = "\
# comment
node_modules/
dist
*.log
/build/
";
        let ig = GitIgnore::parse(content);
        assert!(ig.matches("node_modules/foo.js", false));
        assert!(ig.matches("node_modules", true));
        assert!(ig.matches("dist", false));
        assert!(ig.matches("dist", true));
        assert!(ig.matches("error.log", false));
        assert!(ig.matches("nested/sub/error.log", false));
        assert!(ig.matches("build/out.bin", false));
        assert!(!ig.matches("src/main.rs", false));
    }

    #[test]
    fn test_debouncer_coalescing() {
        let debouncer = Debouncer::new(Duration::from_millis(50));
        assert!(debouncer.push(&["a.rs".to_string()]));
        assert!(!debouncer.push(&["b.rs".to_string()]));

        assert!(debouncer.take_if_ready().is_none());
        std::thread::sleep(Duration::from_millis(60));

        let batch = debouncer.take_if_ready().expect("ready batch");
        assert_eq!(batch, vec!["a.rs", "b.rs"]);
    }
}
