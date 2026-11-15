# Implementation Plan: Rig Phase 3 - Agent Process Lifecycle & Multiplexer Execution

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun engine eksekusi dan siklus hidup proses agen (`launcher.rs`), pengawas proses agen aktif (`supervisor.rs`), listener inbox asinkron dengan blocking wait (`mailbox.rs`), dan subperintah CLI `rig launch`, `rig stop`, `rig ps`, serta `rig inbox --wait` di dalam `crates/temp-terminal-rig`.

**Architecture:** Memperluas crate `temp-terminal-rig` dengan komponen eksekusi proses agen terpadu:
1. `launcher.rs`: Mengintegrasikan `HarnessSpec`, `LayoutAllocator`, dan `WorktreeManager` untuk mempersiapkan konteks eksekusi (`LaunchContext`), mengekspansi argumen command-line, mencatat `Run` ke dalam database SQLite (`rig.db`), dan meluncurkan proses agen baik dalam mode interaktif (pane/tab) maupun headless background dengan redirection stdout/stderr ke ring buffer / log file.
2. `supervisor.rs`: Mengawasi proses agen yang sedang berjalan, melacak PID dan status liveness, memperbarui status run ke database saat proses selesai (`Exited(code)`, `Terminated`), dan memicu injeksi notifikasi 12 baris tail screen dari `StreamWatcher` saat agen berhenti tanpa lapor.
3. `mailbox.rs`: Menyediakan mekanisme `wait_for_inbox` dengan backoff polling efisien untuk mendengarkan pesan baru tanpa membebani CPU, serta integrasi injeksi pesan hening ke antrean penerima.
4. CLI Integration: Subperintah `rig launch`, `rig stop`, `rig ps`, dan penambahan flag `--wait` pada `rig inbox`.

**Tech Stack:** Rust 2021, `rusqlite` (bundled), `regex`, `chrono`, standard library process/thread/sync APIs, Windows process handling (`CREATE_NO_WINDOW`).

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh dimodifikasi.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-rig/`.
- DILARANG menggunakan karakter em dash (\u2014) di seluruh antarmuka, komentar kode, dan dokumentasi (Anti-Slop R-02). Gunakan tanda pisah `-`, titik dua `:`, atau titik tengah `·`.
- DILARANG menggunakan literal emoji sebagai dekorasi UI atau log teks (Anti-Slop R-04).
- Penanganan proses Windows: Eksekusi proses latar belakang (headless) wajib menggunakan flag `CREATE_NO_WINDOW` (flag `0x08000000`) dan pelacakan PID yang aman.
- Integritas database: Seluruh perubahan status proses wajib dipersistensikan secara atomik ke tabel `runs` dan `tickets` di SQLite.

---

### Task 1: Agent Process Launcher Engine (`launcher.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/launcher.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Modify: `crates/temp-terminal-rig/src/error.rs`
- Create: `crates/temp-terminal-rig/tests/launcher_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct LaunchRequest {
      pub ticket_id: String,
      pub role: RunRole,
      pub harness: Option<String>,
      pub mode: Option<RunMode>,
      pub resume: bool,
      pub prompt: Option<String>,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct PreparedLaunch {
      pub ticket_id: String,
      pub run_id: u32,
      pub role: RunRole,
      pub harness: String,
      pub mode: RunMode,
      pub working_dir: std::path::PathBuf,
      pub argv: Vec<String>,
      pub slot_location: TabSlotLocation,
      pub bounds: RectBounds,
  }

  pub struct ProcessLauncher;

  impl ProcessLauncher {
      pub fn prepare_launch(
          db: &mut RigDatabase,
          repo_root: &std::path::Path,
          request: &LaunchRequest,
          layout_config: &LayoutConfig,
      ) -> Result<PreparedLaunch, RigError>;

      pub fn spawn_headless(
          prepared: &PreparedLaunch,
          log_path: Option<&std::path::Path>,
      ) -> Result<u32, RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for ProcessLauncher**
  Buat `crates/temp-terminal-rig/tests/launcher_tests.rs` yang menguji:
  - Persiapan peluncuran agen worker (`prepare_launch`):
    - Mengambil worktree path dari tiket (atau direktori repositori jika worktree kosong).
    - Menghitung argumen command-line via `HarnessSpec::expand_argv`.
    - Mengalokasikan slot layout via `LayoutAllocator::allocate_slot`.
    - Mencatat record baru di tabel `runs` database dengan status `Running`.
  - Eksekusi proses headless (`spawn_headless`):
    - Menjalankan perintah non-blocking (misal `cmd /c echo hello` di Windows).
    - Memverifikasi pengembalian PID proses yang valid (> 0).
    - Memverifikasi penulisan log keluaran ke file log jika `log_path` disediakan.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test launcher_tests`
  Ekspektasi: Gagal kompilasi karena modul `launcher` belum ada.

- [ ] **Step 3: Implement ProcessLauncher in `launcher.rs`**
  - Definisikan `LaunchRequest` dan `PreparedLaunch` lengkap dengan serde trait derives.
  - Implementasikan `prepare_launch`:
    1. Ambil tiket dari database (`db.get_ticket(&request.ticket_id)`).
    2. Tentukan harness dari request, atau dari settings database (`worker.harness` / `reviewer.harness`), atau default `claude`.
    3. Tentukan mode eksekusi dari request atau settings `mode`.
    4. Hitung argumen via `HarnessSpec::expand_argv`.
    5. Hitung slot layout via `LayoutAllocator::allocate_slot`.
    6. Buat record baru di `runs` via `db.create_run`.
  - Implementasikan `spawn_headless`:
    1. Konfigurasikan `std::process::Command`.
    2. Terapkan flag Windows `CREATE_NO_WINDOW` (flag `0x08000000`).
    3. Hubungkan stdout dan stderr ke file log atau piped buffer.
    4. Eksekusi `spawn()` dan kembalikan `child.id()`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test launcher_tests`
  Ekspektasi: Seluruh pengujian peluncur proses lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/launcher.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/src/error.rs crates/temp-terminal-rig/tests/launcher_tests.rs
  git commit -m "feat(rig): implement agent process launcher and execution context engine"
  ```

---

### Task 2: Process Supervisor & Liveness Engine (`supervisor.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/supervisor.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/supervisor_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum ProcessLiveness {
      Running(u32),
      Exited(i32),
      Terminated,
      NotFound,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct SupervisedProcessInfo {
      pub run_id: u32,
      pub ticket_id: String,
      pub pid: u32,
      pub role: RunRole,
      pub harness: String,
      pub liveness: ProcessLiveness,
      pub started_at: String,
  }

  pub struct ProcessSupervisor;

  impl ProcessSupervisor {
      pub fn check_pid_liveness(pid: u32) -> ProcessLiveness;
      pub fn terminate_pid(pid: u32, force: bool) -> Result<(), RigError>;
      pub fn poll_active_runs(db: &mut RigDatabase) -> Result<Vec<SupervisedProcessInfo>, RigError>;
      pub fn handle_unprompted_stop(
          db: &mut RigDatabase,
          run_id: u32,
          ticket_id: &str,
          role: &str,
          tail_lines: &[String],
      ) -> Result<(), RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for ProcessSupervisor**
  Buat `crates/temp-terminal-rig/tests/supervisor_tests.rs` yang menguji:
  - Pemeriksaan liveness PID (`check_pid_liveness`): menguji proses hidup vs PID fiktif/mati.
  - Terminasi proses (`terminate_pid`): menghentikan proses latar belakang secara paksa.
  - Polling active runs (`poll_active_runs`): mendeteksi proses yang telah selesai dan memperbarui status di tabel `runs` menjadi `exited`.
  - Penanganan unprompted stop (`handle_unprompted_stop`): memformat notifikasi dan mengirimkannya ke tabel `messages` dengan penerima `manager`.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test supervisor_tests`
  Ekspektasi: Gagal kompilasi karena modul `supervisor` belum ada.

- [ ] **Step 3: Implement ProcessSupervisor in `supervisor.rs`**
  - Implementasikan pengecekan PID Windows (menggunakan Windows API atau evaluasi exit code proses).
  - Implementasikan `terminate_pid`:
    - Pada Windows, jalankan eksekusi terminasi bersih atau `taskkill /PID <pid> /F`.
  - Implementasikan `poll_active_runs`:
    - Cari seluruh run aktif dari tabel `runs` di database.
    - Cek status PID setiap run.
    - Jika proses telah selesai, perbarui status run di database menjadi `exited`.
  - Implementasikan `handle_unprompted_stop`:
    - Panggil `StreamWatcher::format_unprompted_stop_notice`.
    - Kirim pesan ke database via `db.send_message` ke penerima `manager`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test supervisor_tests`
  Ekspektasi: Seluruh pengujian supervisi proses lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/supervisor.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/supervisor_tests.rs
  git commit -m "feat(rig): implement process supervisor and unprompted stop notification handler"
  ```

---

### Task 3: Inbox Wait & Silent Mailbox Delivery (`mailbox.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/mailbox.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/mailbox_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct InboxWaitOptions {
      pub recipient: String,
      pub ticket_filter: Option<String>,
      pub story_filter: Option<String>,
      pub timeout_ms: Option<u64>,
      pub poll_interval_ms: u64,
  }

  impl Default for InboxWaitOptions {
      fn default() -> Self {
          Self {
              recipient: "manager".to_string(),
              ticket_filter: None,
              story_filter: None,
              timeout_ms: Some(10_000),
              poll_interval_ms: 250,
          }
      }
  }

  pub struct MailboxListener;

  impl MailboxListener {
      pub fn wait_for_messages(
          db: &mut RigDatabase,
          options: &InboxWaitOptions,
      ) -> Result<Vec<Message>, RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for MailboxListener**
  Buat `crates/temp-terminal-rig/tests/mailbox_tests.rs` yang menguji:
  - Pembacaan pesan langsung saat inbox sudah memiliki pesan belum dibaca (`seen = 0`).
  - Penundaan (*blocking wait*) hingga pesan baru masuk dari thread lain.
  - Timeout saat tidak ada pesan masuk hingga batas waktu terlampaui.
  - Penyaringan pesan berdasarkan `ticket_filter` dan `story_filter`.
  - Penandaan otomatis pesan sebagai telah dibaca (`seen = 1`) setelah dikonsumsi.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test mailbox_tests`
  Ekspektasi: Gagal kompilasi karena modul `mailbox` belum ada.

- [ ] **Step 3: Implement MailboxListener in `mailbox.rs`**
  - Implementasikan `wait_for_messages`:
    - Catat `start_time = std::time::Instant::now()`.
    - Lakukan loop:
      1. Baca pesan unread via `db.read_inbox(&options.recipient)`.
      2. Terapkan filter tiket dan story jika disediakan.
      3. Jika ditemukan pesan: tandai `seen = 1` via `db.mark_messages_seen` dan return pesan.
      4. Periksa batas timeout: jika terlampaui, kembalikan `Ok(vec![])`.
      5. Jeda tidur sejenak (`thread::sleep(Duration::from_millis(options.poll_interval_ms))`).

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test mailbox_tests`
  Ekspektasi: Seluruh pengujian inbox wait lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/mailbox.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/mailbox_tests.rs
  git commit -m "feat(rig): implement mailbox wait listener and silent message delivery"
  ```

---

### Task 4: CLI Subcommands for Lifecycle (`rig launch`, `rig stop`, `rig ps`, `rig inbox --wait`)

**Files:**
- Modify: `crates/temp-terminal-rig/src/bin/rig.rs`
- Modify: `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`

**Interfaces:**
- Ekstensi subperintah CLI:
  - `rig launch <ticket_id> [--role <worker|reviewer|manager>] [--harness <harness>] [--mode <pane|tab|headless>] [--resume] [--prompt <text>]`
  - `rig stop <run_id_or_pid> [--force]`
  - `rig ps [--json]`
  - `rig inbox <recipient> [--wait] [--timeout <ms>] [--ticket <id>] [--story <story>]`

- [ ] **Step 1: Write integration tests for new CLI subcommands**
  Tambahkan pengujian di `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`:
  - `rig launch` dalam mode headless memverifikasi spawn proses dan record run baru di database.
  - `rig ps` menampilkan daftar proses aktif dengan kolom status dan role.
  - `rig stop` menghentikan proses agen yang sedang berjalan.
  - `rig inbox manager --wait --timeout 500` memvalidasi timeout dan penerimaan pesan.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Gagal mengenali subperintah `launch`, `stop`, `ps`, atau flag `--wait`.

- [ ] **Step 3: Implement new subcommands in `src/bin/rig.rs`**
  - Pasang handler argument parsing untuk `launch`.
  - Pasang handler untuk `stop`.
  - Pasang handler untuk `ps`.
  - Perluas handler `inbox` dengan dukungan flag `--wait`, `--timeout`, `--ticket`, dan `--story`.
  - Perbarui dokumentasi bantuan pada `print_help()`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Seluruh pengujian CLI lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/bin/rig.rs crates/temp-terminal-rig/tests/e2e_cli_tests.rs
  git commit -m "feat(rig): wire up launch, stop, ps, and inbox wait commands in rig cli"
  ```

---

### Task 5: End-to-End Integration Verification & Anti-Slop Audit

**Files:**
- Create: `crates/temp-terminal-rig/tests/phase3_integration_tests.rs`
- Run: Full test suite & workspace checks.

- [ ] **Step 1: Write full lifecycle Phase 3 integration test**
  Uji alur terpadu lengkap:
  1. Pembuatan tiket di database.
  2. Peluncuran agen worker via `ProcessLauncher::prepare_launch`.
  3. Eksekusi headless dan pencatatan PID aktif.
  4. Pengawasan status liveness via `ProcessSupervisor`.
  5. Pengiriman pesan antar-agen dan penerimaan via `MailboxListener::wait_for_messages`.
  6. Simulasi deteksi turn settlement dan unprompted stop notification.
  7. Terminasi proses bersih via `ProcessSupervisor::terminate_pid`.
  8. Verifikasi konsistensi status akhir di database.

- [ ] **Step 2: Run all tests in temp-terminal-rig and whole workspace**
  Jalankan:
  ```bash
  cargo test -p temp-terminal-rig
  cargo check --workspace --all-targets
  ```
  Ekspektasi: Semua pengujian lulus, nol error, nol warning.

- [ ] **Step 3: Verify Anti-Slop Compliance**
  Lakukan audit otomatis:
  - Nol karakter em dash (\u2014) di seluruh kode baru.
  - Nol emoji dekoratif.

- [ ] **Step 4: Commit**
  ```bash
  git add crates/temp-terminal-rig/tests/phase3_integration_tests.rs
  git commit -m "test(rig): add phase 3 end-to-end integration test and finalize validation"
  ```
