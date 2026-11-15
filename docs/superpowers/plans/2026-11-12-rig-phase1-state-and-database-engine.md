# Implementation Plan: Rig Phase 1 - State & Database Engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun crate baru `crates/temp-terminal-rig` sebagai fondasi state dan database engine untuk orkestrasi agen multi-peran (Manager, Worker, Reviewer, Orchestrator) secara 100% native Rust dengan SQLite (WAL mode), CRUD tiket terisolasi, resolusi antrean dependensi, model eksekusi proses agen (*harness expansion*), dan bus pesan inter-agen.

**Architecture:** Memisahkan lapisan data ke dalam crate independen `crates/temp-terminal-rig` yang mengelola database SQLite lokal repositori (`.rig/rig.db`) dan database hub global (`~/.rig/hub.db`) menggunakan `rusqlite` (fitur `bundled`). Menyediakan abstraksi tipe data murni (`Ticket`, `Run`, `Message`, `Settings`, `Harness`), resolusi dependensi prasyarat tiket (`after`), ekspansi argumen command-line untuk seluruh harness agen AI (Claude Code, Codex, OpenCode, Pi, OMP, CommandCode), serta mengekspos CLI dasar `rig` untuk pengujian deterministik secara headless.

**Tech Stack:** Rust 2021, `rusqlite` 0.32 (bundled), `serde`, `serde_json`, `thiserror`, `chrono` / timestamps UTC.

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh dimodifikasi.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-rig/` dan integrasi CLI di `temp-terminal/crates/temp-terminal-cli/`.
- DILARANG menggunakan karakter em dash (\u2014) di seluruh antarmuka, komentar kode, dan dokumentasi (Anti-Slop R-02).
- DILARANG menggunakan literal emoji sebagai dekorasi UI atau log teks (Anti-Slop R-04).
- Database SQLite wajib mengaktifkan `PRAGMA busy_timeout = 5000` dan `PRAGMA journal_mode = WAL` untuk menjamin konkurensi aman multi-proses.
- Semua penomoran tiket harus thread-safe dan inkremental per repositori (`T-1`, `T-2`, dst.).

---

### Task 1: Crate Setup & Lapisan Akses SQLite Database (`temp-terminal-rig`)

**Files:**
- Create: `crates/temp-terminal-rig/Cargo.toml`
- Create: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/src/error.rs`
- Create: `crates/temp-terminal-rig/src/db.rs`
- Modify: `Cargo.toml` (tambahkan member workspace)
- Create: `crates/temp-terminal-rig/tests/db_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum RigError {
      Database(rusqlite::Error),
      Io(std::io::Error),
      TicketNotFound(String),
      RunNotFound(String),
      InvalidParameter(String),
  }

  pub struct RigDatabase {
      conn: rusqlite::Connection,
  }

  impl RigDatabase {
      pub fn open_repo(repo_root: &std::path::Path) -> Result<Self, RigError>;
      pub fn open_in_memory() -> Result<Self, RigError>;
      pub fn open_hub(hub_home: &std::path::Path) -> Result<Self, RigError>;
      pub fn connection(&self) -> &rusqlite::Connection;
      pub fn connection_mut(&mut self) -> &mut rusqlite::Connection;
  }
  ```

- [ ] **Step 1: Definisikan Cargo.toml untuk crates/temp-terminal-rig dan daftarkan ke workspace**

Buat `crates/temp-terminal-rig/Cargo.toml`:
```toml
[package]
name = "temp-terminal-rig"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = "3.14"
```
Modifikasi workspace `Cargo.toml` root untuk menambahkan `"crates/temp-terminal-rig"` ke daftar `members`.

- [ ] **Step 2: Tulis tes unit yang gagal untuk inisialisasi database SQLite (Red)**

Buat berkas `crates/temp-terminal-rig/tests/db_tests.rs`:
```rust
use temp_terminal_rig::db::RigDatabase;
use tempfile::tempdir;

#[test]
fn test_open_repo_database_creates_tables_and_pragmas() {
    let dir = tempdir().unwrap();
    let db = RigDatabase::open_repo(dir.path()).expect("failed to open repo database");

    // Verifikasi journal mode WAL
    let journal_mode: String = db.connection()
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("failed to get journal_mode");
    assert_eq!(journal_mode.to_lowercase(), "wal");

    // Verifikasi tabel tickets ada
    let ticket_table_count: i64 = db.connection()
        .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='tickets';", [], |row| row.get(0))
        .expect("failed to check tickets table");
    assert_eq!(ticket_table_count, 1);

    // Verifikasi tabel runs, messages, settings
    let tables = ["runs", "messages", "settings"];
    for table in tables {
        let count: i64 = db.connection()
            .query_row(&format!("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{}';", table), [], |row| row.get(0))
            .unwrap_or(0);
        assert_eq!(count, 1, "table {} missing", table);
    }
}

#[test]
fn test_open_hub_database_creates_hub_tables() {
    let dir = tempdir().unwrap();
    let db = RigDatabase::open_hub(dir.path()).expect("failed to open hub database");

    let tables = ["repos", "stories", "story_repos", "messages", "settings"];
    for table in tables {
        let count: i64 = db.connection()
            .query_row(&format!("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{}';", table), [], |row| row.get(0))
            .unwrap_or(0);
        assert_eq!(count, 1, "hub table {} missing", table);
    }
}
```

- [ ] **Step 3: Jalankan tes unit untuk memastikan kegagalan kompilasi (Red)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test db_tests
```
Verifikasi kegagalan karena modul dan fungsi belum diimplementasikan.

- [ ] **Step 4: Implementasikan error.rs, db.rs, dan lib.rs (Green)**

Implementasikan skema lengkap `rig.db` dan `hub.db` pada `crates/temp-terminal-rig/src/db.rs` dengan `PRAGMA busy_timeout = 5000` dan `PRAGMA journal_mode = WAL`.

- [ ] **Step 5: Jalankan tes unit untuk memastikan sukses (Pass)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test db_tests
```
Pastikan seluruh tes lolos.

- [ ] **Step 6: Commit Task 1**

```powershell
git add crates/temp-terminal-rig Cargo.toml
git commit -m "feat(rig): initialize temp-terminal-rig crate and sqlite database engine"
```

---

### Task 2: Model Tiket, CRUD, dan Mesin Resolusi Dependensi Antrean

**Files:**
- Create: `crates/temp-terminal-rig/src/ticket.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/ticket_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum TicketStatus {
      Open,
      Draft,
      Working,
      Reviewing,
      Review,
      NeedsYou,
      Blocked,
      Done,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Ticket {
      pub id: String,
      pub title: String,
      pub body: String,
      pub status: TicketStatus,
      pub manager: String,
      pub review: bool,
      pub branch: Option<String>,
      pub worktree: Option<String>,
      pub created: String,
      pub after: Option<String>,
      pub base: Option<String>,
      pub story: Option<String>,
  }

  pub struct TicketFilter<'a> {
      pub manager: Option<&'a str>,
      pub include_done: bool,
      pub story: Option<&'a str>,
  }

  impl RigDatabase {
      pub fn create_ticket(&self, title: &str, body: &str, manager: &str, review: bool, after: Option<&str>, base: Option<&str>, story: Option<&str>) -> Result<Ticket, RigError>;
      pub fn get_ticket(&self, id: &str) -> Result<Ticket, RigError>;
      pub fn update_ticket_status(&self, id: &str, status: TicketStatus) -> Result<(), RigError>;
      pub fn update_ticket(&self, ticket: &Ticket) -> Result<(), RigError>;
      pub fn list_tickets(&self, filter: &TicketFilter) -> Result<Vec<Ticket>, RigError>;
      pub fn waiting_in(&self, ticket: &Ticket) -> Result<Vec<String>, RigError>;
      pub fn group_of(&self, ticket: &Ticket) -> Result<&'static str, RigError>;
  }
  ```

- [ ] **Step 1: Tulis tes unit yang gagal untuk model tiket dan resolusi dependensi (Red)**

Buat `crates/temp-terminal-rig/tests/ticket_tests.rs`:
```rust
use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::ticket::{TicketFilter, TicketStatus};

#[test]
fn test_ticket_crud_and_auto_increment() {
    let db = RigDatabase::open_in_memory().unwrap();

    let t1 = db.create_ticket("First Task", "Goal: A", "manager:p1", false, None, Some("main"), None).unwrap();
    assert_eq!(t1.id, "T-1");
    assert_eq!(t1.title, "First Task");
    assert_eq!(t1.status, TicketStatus::Open);

    let t2 = db.create_ticket("Second Task", "Goal: B", "manager:p1", true, Some("T-1"), Some("main"), None).unwrap();
    assert_eq!(t2.id, "T-2");
    assert_eq!(t2.review, true);
    assert_eq!(t2.after.as_deref(), Some("T-1"));

    // Cek daftar tiket
    let all = db.list_tickets(&TicketFilter { manager: None, include_done: true, story: None }).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_ticket_dependency_queueing_and_group_mapping() {
    let db = RigDatabase::open_in_memory().unwrap();

    let t1 = db.create_ticket("Foundation", "Goal: Base", "manager:p1", false, None, None, None).unwrap();
    let t2 = db.create_ticket("Dependent", "Goal: Layer", "manager:p1", false, Some("T-1"), None, None).unwrap();

    // t1 belum done, sehingga t2 masih Queued
    let waiting = db.waiting_in(&t2).unwrap();
    assert_eq!(waiting, vec!["T-1"]);
    assert_eq!(db.group_of(&t2).unwrap(), "Queued");

    // t1 diubah menjadi done
    db.update_ticket_status(&t1.id, TicketStatus::Done).unwrap();
    let waiting_after = db.waiting_in(&t2).unwrap();
    assert!(waiting_after.is_empty());
    assert_eq!(db.group_of(&t2).unwrap(), "To do");
}
```

- [ ] **Step 2: Jalankan tes unit dan konfirmasi kegagalan (Red)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test ticket_tests
```

- [ ] **Step 3: Implementasikan ticket.rs dan integrasikan ke lib.rs (Green)**

Implementasikan CRUD tiket, penomoran atomic `T-n` via `SELECT MAX(CAST(SUBSTR(id, 3) AS INTEGER))`, fungsi `waiting_in`, dan pengelompokan `group_of` ("Needs you", "Working", "Ready", "Queued", "To do", "Done").

- [ ] **Step 4: Jalankan tes unit dan verifikasi lulus (Pass)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test ticket_tests
```
Pastikan seluruh tes lolos.

- [ ] **Step 5: Commit Task 2**

```powershell
git add crates/temp-terminal-rig
git commit -m "feat(rig): implement ticket model, atomic auto-increment, and dependency queueing"
```

---

### Task 3: Model Run dan Mesin Ekspansi Argumen Harness Agen AI

**Files:**
- Create: `crates/temp-terminal-rig/src/harness.rs`
- Create: `crates/temp-terminal-rig/src/run.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/harness_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum RunRole {
      Worker,
      Reviewer,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum RunMode {
      Pane,
      Tab,
      Headless,
      Space,
      Overflow(u32),
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Run {
      pub id: i64,
      pub ticket: String,
      pub role: RunRole,
      pub harness: String,
      pub model: Option<String>,
      pub effort: Option<String>,
      pub mode: RunMode,
      pub pane: Option<String>,
      pub sid: String,
      pub pid: Option<u32>,
      pub cmd: String,
      pub status: String,
      pub cwd: String,
      pub started: String,
  }

  pub struct HarnessSpec {
      pub kind: Option<&'static str>,
      pub launch: bool,
      pub model_flags: &'static [&'static str],
      pub effort_flags: &'static [&'static str],
      pub interactive_args: &'static [&'static str],
      pub headless_args: &'static [&'static str],
      pub resume_args: &'static [&'static str],
  }

  pub fn expand_argv(
      harness: &str,
      launch_kind: &str, // "interactive", "headless", "resume"
      model: Option<&str>,
      effort: Option<&str>,
      sid: &str,
      prompt: Option<&str>,
  ) -> Result<Vec<String>, RigError>;
  ```

- [ ] **Step 1: Tulis tes unit yang gagal untuk ekspansi argumen harness (Red)**

Buat `crates/temp-terminal-rig/tests/harness_tests.rs`:
```rust
use temp_terminal_rig::harness::expand_argv;

#[test]
fn test_claude_argv_expansion() {
    let argv = expand_argv(
        "claude",
        "headless",
        Some("opus"),
        Some("high"),
        "ses-123",
        Some("run tests"),
    ).unwrap();

    assert_eq!(
        argv,
        vec![
            "claude",
            "-p",
            "--dangerously-skip-permissions",
            "--session-id",
            "ses-123",
            "--model",
            "opus",
            "--effort",
            "high",
            "run tests"
        ]
    );
}

#[test]
fn test_codex_argv_expansion() {
    let argv = expand_argv(
        "codex",
        "interactive",
        Some("gpt-5"),
        Some("high"),
        "ses-456",
        None,
    ).unwrap();

    assert_eq!(
        argv,
        vec![
            "codex",
            "--dangerously-bypass-approvals-and-sandbox",
            "-m",
            "gpt-5",
            "-c",
            "model_reasoning_effort=high",
        ]
    );
}

#[test]
fn test_pi_and_opencode_argv_expansion() {
    let pi_argv = expand_argv("pi", "headless", None, None, "ses-pi", Some("hello")).unwrap();
    assert_eq!(pi_argv, vec!["pi", "-p", "--session-id", "ses-pi", "hello"]);

    let opencode_argv = expand_argv("opencode", "interactive", Some("gemini"), Some("fast"), "ses-oc", None).unwrap();
    assert_eq!(opencode_argv, vec!["opencode", "--auto", "-s", "ses_ses-oc", "--prompt"]);
}
```

- [ ] **Step 2: Jalankan tes unit dan konfirmasi kegagalan (Red)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test harness_tests
```

- [ ] **Step 3: Implementasikan harness.rs, run.rs, dan metode database terkait (Green)**

Implementasikan spesifikasi harness untuk Claude, Codex, OpenCode, Pi, OMP, dan CommandCode dengan penggantian token `{model}`, `{effort}`, dan `{sid}`.
Implementasikan CRUD entri `runs` pada `RigDatabase`.

- [ ] **Step 4: Jalankan tes unit dan verifikasi lulus (Pass)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test harness_tests
```
Pastikan seluruh tes lolos.

- [ ] **Step 5: Commit Task 3**

```powershell
git add crates/temp-terminal-rig
git commit -m "feat(rig): implement run tracking and harness commandline expansion"
```

---

### Task 4: Bus Pesan Inter-Agen, Pengiriman Hening, dan Konfigurasi Settings

**Files:**
- Create: `crates/temp-terminal-rig/src/message.rs`
- Create: `crates/temp-terminal-rig/src/settings.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/message_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Message {
      pub id: i64,
      pub ticket: Option<String>,
      pub frm: String,
      pub too: String,
      pub body: String,
      pub seen: bool,
      pub at: String,
      pub story: Option<String>,
  }

  impl RigDatabase {
      pub fn send_message(&self, ticket: Option<&str>, frm: &str, too: &str, body: &str, story: Option<&str>) -> Result<i64, RigError>;
      pub fn read_inbox(&self, recipient: &str, mark_seen: bool, story: Option<&str>) -> Result<Vec<Message>, RigError>;
      pub fn mark_messages_seen(&self, ids: &[i64]) -> Result<(), RigError>;
      pub fn get_setting(&self, key: &str) -> Result<Option<String>, RigError>;
      pub fn get_setting_or_default(&self, key: &str) -> Result<String, RigError>;
      pub fn set_setting(&self, key: &str, value: &str) -> Result<(), RigError>;
      pub fn list_settings(&self) -> Result<std::collections::HashMap<String, String>, RigError>;
  }
  ```

- [ ] **Step 1: Tulis tes unit yang gagal untuk perpesanan dan settings (Red)**

Buat `crates/temp-terminal-rig/tests/message_tests.rs`:
```rust
use temp_terminal_rig::db::RigDatabase;

#[test]
fn test_message_sending_and_inbox_consumption() {
    let db = RigDatabase::open_in_memory().unwrap();

    let msg_id = db.send_message(Some("T-1"), "T-1/worker", "manager:p1", "done: all tests green", None).unwrap();
    assert!(msg_id > 0);

    // Baca unread inbox tanpa menandai seen
    let unread = db.read_inbox("manager:p1", false, None).unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].body, "done: all tests green");
    assert_eq!(unread[0].ticket.as_deref(), Some("T-1"));

    // Baca inbox dan tandai seen
    let unread_marked = db.read_inbox("manager:p1", true, None).unwrap();
    assert_eq!(unread_marked.len(), 1);

    // Pembacaan berikutnya harus kosong
    let empty = db.read_inbox("manager:p1", false, None).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn test_settings_persistence_and_defaults() {
    let db = RigDatabase::open_in_memory().unwrap();

    // Default setting fallback
    assert_eq!(db.get_setting_or_default("worker.harness").unwrap(), "claude");
    assert_eq!(db.get_setting_or_default("merge").unwrap(), "local");

    // Override setting
    db.set_setting("worker.harness", "codex").unwrap();
    assert_eq!(db.get_setting_or_default("worker.harness").unwrap(), "codex");
}
```

- [ ] **Step 2: Jalankan tes unit dan konfirmasi kegagalan (Red)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test message_tests
```

- [ ] **Step 3: Implementasikan message.rs, settings.rs, dan integrasikan ke RigDatabase (Green)**

Implementasikan penyimpanan pesan, query inbox berurutan, penandaan status `seen`, dan kamus nilai default setting (`worker.harness=claude`, `reviewer.harness=codex`, `mode=pane`, `worktree=on`, `autonomy=review`, `merge=local`).

- [ ] **Step 4: Jalankan tes unit dan verifikasi lulus (Pass)**

Jalankan:
```powershell
cargo test -p temp-terminal-rig --test message_tests
```
Pastikan seluruh tes lolos.

- [ ] **Step 5: Commit Task 4**

```powershell
git add crates/temp-terminal-rig
git commit -m "feat(rig): implement inter-agent message bus and persistent settings store"
```

---

### Task 5: Integrasi CLI Sub-Perintah `rig` dan Pengujian Alur End-to-End

**Files:**
- Create: `crates/temp-terminal-rig/src/bin/rig.rs`
- Modify: `crates/temp-terminal-rig/Cargo.toml`
- Create: `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`

**Interfaces:**
- Produces: Binary executable `rig` (atau subperintah CLI) yang dapat dipanggil langsung dari terminal untuk pengujian end-to-end:
  - `rig new "title" [--body text|-] [--review] [--after T-1] [--base branch]`
  - `rig edit <T-n> [status=...] [title=...]`
  - `rig ls [--all] [--mine]`
  - `rig show <T-n>`
  - `rig set <k> <v>`
  - `rig get [k]`
  - `rig say <too> <msg> [--ticket T-n]`
  - `rig inbox [--wait]`

- [ ] **Step 1: Tulis tes integrasi end-to-end simulasi alur kerja lengkap (Red)**

Buat `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`:
```rust
use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::ticket::{TicketFilter, TicketStatus};
use tempfile::tempdir;

#[test]
fn test_end_to_end_rig_lifecycle_flow() {
    let dir = tempdir().unwrap();
    let db = RigDatabase::open_repo(dir.path()).unwrap();

    // 1. Simpan konfigurasi awal repo
    db.set_setting("worker.harness", "claude").unwrap();
    db.set_setting("reviewer.harness", "codex").unwrap();
    assert_eq!(db.get_setting_or_default("worker.harness").unwrap(), "claude");

    // 2. Buat tiket pertama T-1
    let t1 = db.create_ticket("Build Database Engine", "Goal: SQLite engine", "manager:p1", true, None, Some("main"), None).unwrap();
    assert_eq!(t1.id, "T-1");
    assert_eq!(db.group_of(&t1).unwrap(), "To do");

    // 3. Buat tiket kedua T-2 yang bergantung pada T-1
    let t2 = db.create_ticket("Build TUI Board", "Goal: Render board", "manager:p1", false, Some("T-1"), Some("main"), None).unwrap();
    assert_eq!(t2.id, "T-2");
    assert_eq!(db.group_of(&t2).unwrap(), "Queued");

    // 4. Update status T-1 menjadi working
    db.update_ticket_status(&t1.id, TicketStatus::Working).unwrap();
    assert_eq!(db.group_of(&t1).unwrap(), "Working");

    // 5. Worker mengirim pesan progres
    db.send_message(Some(&t1.id), "T-1/worker", "manager:p1", "milestone: schema completed", None).unwrap();
    let msgs = db.read_inbox("manager:p1", true, None).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].body, "milestone: schema completed");

    // 6. Worker selesai dan menyerahkan untuk review
    db.update_ticket_status(&t1.id, TicketStatus::Review).unwrap();
    assert_eq!(db.group_of(&t1).unwrap(), "Ready");

    // 7. Reviewer PASS dan Manager menandai Done
    db.update_ticket_status(&t1.id, TicketStatus::Done).unwrap();
    assert_eq!(db.group_of(&t1).unwrap(), "Done");

    // 8. T-2 otomatis terbebas dari antrean
    assert_eq!(db.group_of(&t2).unwrap(), "To do");
}
```

- [ ] **Step 2: Implementasikan binary CLI `rig.rs` pada crates/temp-terminal-rig (Green)**

Implementasikan parser argumen CLI (`init`, `new`, `edit`, `ls`, `show`, `set`, `get`, `say`, `inbox`) yang berinteraksi langsung dengan `RigDatabase`.

- [ ] **Step 3: Jalankan seluruh suite tes unit dan integrasi temp-terminal-rig**

Jalankan:
```powershell
cargo test -p temp-terminal-rig
```
Pastikan seluruh pengujian (db, ticket, harness, message, e2e) lulus 100%.

- [ ] **Step 4: Jalankan verifikasi workspace secara menyeluruh**

Jalankan:
```powershell
cargo check --workspace --all-targets
```
Pastikan 0 errors dan 0 warnings di seluruh workspace.

- [ ] **Step 5: Audit kepatuhan Anti-Slop (Nol Em Dash)**

Jalankan pemindaian otomatis terhadap karakter em dash (\u2014) di `crates/temp-terminal-rig/`. Pastikan 0 kecocokan.

- [ ] **Step 6: Commit Task 5**

```powershell
git add crates/temp-terminal-rig
git commit -m "feat(rig): implement rig cli binary, end-to-end integration tests, and finalize phase 1"
```
