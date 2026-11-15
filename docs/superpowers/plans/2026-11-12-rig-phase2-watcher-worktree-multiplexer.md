# Implementation Plan: Rig Phase 2 - Watcher, Worktree Isolation & Multiplexer Layout

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun engine isolasi Git Worktree, sistem pengawasan hening (*silent watcher*) untuk deteksi turn settlement dan ekstraksi 12-line screen tail buffer ConPTY/VT, serta alokasi layout multiplexer (*slot engine*) untuk multi-pane dan overflow tabs di dalam `crates/temp-terminal-rig`.

**Architecture:** Memperluas crate `temp-terminal-rig` dengan tiga subsistem inti:
1. `worktree.rs`: Git worktree automation (`.rig/wt/T-n`), verifikasi landing berbasis merge-base diff (`has_landed`), squash merge runner, dan safe cleanup dengan retry fallback terhadap file-locking Windows.
2. `watcher.rs`: VT stream settlement detector yang mengenali status `idle`, `blocked`, `working`, dan `exited` dari berbagai harness (Claude Code, Codex, Pi, OpenCode, OMP), serta mengekstrak 12 baris terakhir buffer terminal saat agen berhenti tanpa lapor.
3. `layout.rs`: Algoritma alokasi layout adaptif `slot()` (Manager kiri 50%, Board kanan-atas, Worker runs kanan-bawah bertumpuk hingga `pane.max`, dan overflow tabs `over-1`, `over-2` dengan grid 2-kolom seimbang hingga `tab.max` = 8).
4. Integrasi CLI `rig worktree`, `rig land`, `rig watch`, dan `rig slot`.

**Tech Stack:** Rust 2021, `regex`, `rusqlite` (bundled), `tempfile`, `chrono`, standard library process/path APIs.

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh disentuh.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-rig/`.
- DILARANG menggunakan karakter em dash (\u2014) di seluruh antarmuka, komentar kode, dan dokumentasi (Anti-Slop R-02). Gunakan tanda pisah `-`, titik dua `:`, atau titik tengah `·`.
- DILARANG menggunakan literal emoji sebagai dekorasi UI atau log teks (Anti-Slop R-04).
- Keamanan operasi Git pada Windows: Operasi penghapusan direktori worktree harus menangani file locking sementara dengan mekanisme retry bertahap (hingga 10 kali @ 500ms).
- Isolasi cabang: Branch worktree wajib menggunakan konvensi nama `rig/T-n` di dalam direktori `.rig/wt/T-n`.

---

### Task 1: Git Worktree Engine & Landing Safety (`worktree.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/worktree.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Modify: `crates/temp-terminal-rig/src/error.rs`
- Create: `crates/temp-terminal-rig/tests/worktree_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct WorktreeInfo {
      pub ticket_id: String,
      pub branch: String,
      pub path: std::path::PathBuf,
      pub is_clean: bool,
  }

  pub struct WorktreeManager;

  impl WorktreeManager {
      pub fn create_worktree(repo_root: &std::path::Path, ticket_id: &str, base_branch: &str) -> Result<std::path::PathBuf, RigError>;
      pub fn has_landed(repo_root: &std::path::Path, base_branch: &str, ticket_branch: &str) -> bool;
      pub fn land_worktree(repo_root: &std::path::Path, ticket_id: &str, base_branch: &str, commit_message: Option<&str>) -> Result<String, RigError>;
      pub fn clean_worktree(repo_root: &std::path::Path, ticket_id: &str, base_branch: &str, force: bool) -> Result<(), RigError>;
      pub fn list_worktrees(repo_root: &std::path::Path) -> Result<Vec<WorktreeInfo>, RigError>;
  }
  ```

- [ ] **Step 1: Write the failing tests for WorktreeManager**
  Buat `crates/temp-terminal-rig/tests/worktree_tests.rs` yang menguji:
  - Pembuatan worktree terisolasi di `.rig/wt/T-1` dengan branch `rig/T-1`.
  - Verifikasi landing saat ada file baru di-commit pada branch dan di-merge ke base.
  - Penolakan pembersihan jika branch belum mendarat (`has_landed == false`) tanpa `--force`.
  - Pembersihan sukses dengan flag `--force` atau setelah mendarat.
  - Simulasi listing worktree aktif.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test worktree_tests`
  Ekspektasi: Gagal kompilasi karena modul `worktree` dan fungsi belum ada.

- [ ] **Step 3: Implement WorktreeManager in `worktree.rs`**
  - Implementasikan `run_git_cmd` dan `run_git_status` sebagai helper eksekusi Git.
  - Implementasikan `create_worktree`: mengeksekusi `git worktree add <path> -b <branch> <base>`.
  - Implementasikan `has_landed`:
    1. Cari `merge-base <base> <branch>`.
    2. Dapatkan file terubah melalui `git diff --name-only <merge-base> <branch>`.
    3. Jika tidak ada file berubah, return `true`.
    4. Periksa apakah `git diff --quiet <branch> <base> -- <files>` mengembalikan status 0.
  - Implementasikan `land_worktree`:
    1. Lakukan `git merge --squash <branch>` ke dalam `base_branch` atau commit langsung.
    2. Jalankan `has_landed` untuk memastikan landing tervalidasi.
  - Implementasikan `clean_worktree`:
    1. Validasi `has_landed` (jika bukan `force`, return `RigError::UnmergedWorktree`).
    2. Eksekusi `git worktree remove --force <path>`.
    3. Windows locking fallback: loop hingga 10 kali dengan jeda 500ms untuk `remove_dir_all`.
    4. Eksekusi `git branch -D <branch>`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test worktree_tests`
  Ekspektasi: Seluruh pengujian worktree lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/worktree.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/src/error.rs crates/temp-terminal-rig/tests/worktree_tests.rs
  git commit -m "feat(rig): implement git worktree manager and merge-base landing safety"
  ```

---

### Task 2: Terminal VT Stream Watcher & 12-Line Screen Tail Capture (`watcher.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/watcher.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Modify: `crates/temp-terminal-rig/Cargo.toml` (tambahkan dependency `regex`)
- Create: `crates/temp-terminal-rig/tests/watcher_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  pub enum SettlementKind {
      Idle,
      Blocked,
      Working,
      Exited,
  }

  pub struct StreamWatcher;

  impl StreamWatcher {
      pub fn strip_ansi(raw: &str) -> String;
      pub fn extract_screen_tail(raw_output: &str, line_count: usize) -> Vec<String>;
      pub fn detect_settlement(raw_output: &str, harness: &str) -> SettlementKind;
      pub fn format_unprompted_stop_notice(run_id: u32, role: &str, screen_tail: &[String]) -> String;
  }
  ```

- [ ] **Step 1: Write the failing tests for StreamWatcher**
  Buat `crates/temp-terminal-rig/tests/watcher_tests.rs` yang menguji:
  - Pembersihan kode ANSI (warna, kursor, OSC escape sequences).
  - Ekstraksi tepat 12 baris terakhir dari buffer teks terminal.
  - Deteksi prompt `Idle` untuk Claude Code (`❯`, prompt menunggu), Codex (`› `), dan generic shell (`$ `).
  - Deteksi status `Blocked` (misal pertanyaan konfirmasi `[y/N]`, `permission required`, `Allow this action?`).
  - Deteksi status `Working` (output sedang aktif / streaming log).
  - Format pesan laporan unprompted stop: `"[watch] worker run #1 is idle without reporting. Screen:\n<tail_content>"`.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test watcher_tests`
  Ekspektasi: Gagal kompilasi karena modul `watcher` belum didefinisikan.

- [ ] **Step 3: Implement StreamWatcher in `watcher.rs`**
  - Implementasikan pembersihan kode escape ANSI dengan regex teroptimasi.
  - Implementasikan `extract_screen_tail`: memotong baris kosong di akhir, mengambil maksimal `line_count` (default 12) baris terbawah.
  - Implementasikan `detect_settlement`:
    - Pencocokan regex spesifik per harness agen (`claude`, `codex`, `pi`, `opencode`, `omp`).
    - Deteksi pattern blocked / prompt konfirmasi.
  - Implementasikan generator notifikasi terformat untuk dikirim ke inbox Manager.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test watcher_tests`
  Ekspektasi: Seluruh pengujian watcher dan ekstraksi 12 baris lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/watcher.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/Cargo.toml crates/temp-terminal-rig/tests/watcher_tests.rs
  git commit -m "feat(rig): implement terminal stream settlement watcher and screen tail extractor"
  ```

---

### Task 3: Multiplexer Layout & Slot Allocation Engine (`layout.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/layout.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/layout_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum SlotRole {
      Manager,
      Board,
      Worker(u32),
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct TabSlotLocation {
      pub tab_name: String,
      pub slot_index: u32,
      pub row: u32,
      pub col: u32,
      pub total_rows: u32,
      pub total_cols: u32,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct RectBounds {
      pub x: f32,
      pub y: f32,
      pub w: f32,
      pub h: f32,
  }

  pub struct LayoutConfig {
      pub pane_max: u32,
      pub tab_max: u32,
  }

  pub struct LayoutAllocator;

  impl LayoutAllocator {
      pub fn allocate_slot(role: &SlotRole, config: &LayoutConfig) -> TabSlotLocation;
      pub fn compute_rect_bounds(location: &TabSlotLocation) -> RectBounds;
  }
  ```

- [ ] **Step 1: Write the failing tests for LayoutAllocator**
  Buat `crates/temp-terminal-rig/tests/layout_tests.rs` yang menguji:
  - Alokasi `SlotRole::Manager`: Tab `main`, menempati setengah sisi kiri (`x=0.0, y=0.0, w=0.5, h=1.0`).
  - Alokasi `SlotRole::Board`: Tab `main`, menempati kanan atas (`x=0.5, y=0.0, w=0.5, h=0.5`).
  - Alokasi `SlotRole::Worker(0)`: Tab `main`, kanan bawah slot 1 (`x=0.5, y=0.5, w=0.5, h=0.5` jika 1 worker).
  - Alokasi `SlotRole::Worker(1)`: Membagi ruang kanan bawah bersama Worker 0.
  - Alokasi `SlotRole::Worker(2)` (saat `pane_max = 2`): Dialokasikan ke overflow tab `over-1`.
  - Alokasi grid 2-kolom seimbang pada tab overflow hingga `tab_max = 8`.
  - Alokasi `SlotRole::Worker(10)`: Dialokasikan ke overflow tab `over-2`.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test layout_tests`
  Ekspektasi: Gagal kompilasi karena modul `layout` belum ada.

- [ ] **Step 3: Implement LayoutAllocator in `layout.rs`**
  - Implementasikan `LayoutConfig` dengan default `pane_max = 2` dan `tab_max = 8`.
  - Implementasikan pemetaan `allocate_slot` sesuai spesifikasi herdr-rig asli.
  - Implementasikan `compute_rect_bounds` untuk menghitung koordinat normalized bounding box [0.0, 1.0].

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test layout_tests`
  Ekspektasi: Seluruh pengujian alokasi layout dan geometri bounding box lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/layout.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/layout_tests.rs
  git commit -m "feat(rig): implement multiplexer layout allocator and overflow grid calculations"
  ```

---

### Task 4: CLI Subcommands Integration (`rig worktree`, `rig land`, `rig watch`, `rig slot`)

**Files:**
- Modify: `crates/temp-terminal-rig/src/bin/rig.rs`
- Modify: `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`

**Interfaces:**
- Ekstensi subperintah CLI:
  - `rig worktree create <ticket_id> [--base <branch>]`
  - `rig worktree clean <ticket_id> [--force]`
  - `rig worktree ls`
  - `rig land <ticket_id> [--message <msg>]`
  - `rig watch <ticket_id> [--lines <n>]`
  - `rig slot <role>`

- [ ] **Step 1: Write integration tests for new CLI subcommands**
  Tambahkan pengujian di `crates/temp-terminal-rig/tests/e2e_cli_tests.rs` yang mengeksekusi subperintah baru dan memverifikasi keluaran stdout/stderr serta exit code.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Gagal mengenali subperintah baru.

- [ ] **Step 3: Implement new subcommands in `src/bin/rig.rs`**
  - Pasang handler argument parsing untuk `worktree` (`create`, `clean`, `ls`).
  - Pasang handler untuk `land`.
  - Pasang handler untuk `watch`.
  - Pasang handler untuk `slot`.
  - Hubungkan dengan modul `worktree`, `watcher`, dan `layout`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Seluruh pengujian subperintah CLI lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/bin/rig.rs crates/temp-terminal-rig/tests/e2e_cli_tests.rs
  git commit -m "feat(rig): wire up worktree, land, watch, and slot commands in rig cli"
  ```

---

### Task 5: End-to-End Integration Verification & Anti-Slop Audit

**Files:**
- Create: `crates/temp-terminal-rig/tests/phase2_integration_tests.rs`
- Run: Full test suite & workspace checks.

- [ ] **Step 1: Write full lifecycle Phase 2 integration test**
  Uji alur lengkap:
  1. Pembuatan tiket via `RigDatabase`.
  2. Pembuatan worktree terisolasi via `WorktreeManager`.
  3. Simulasi perubahan kode pada worktree.
  4. Deteksi status buffer output via `StreamWatcher`.
  5. Verifikasi ekstraksi 12 baris tail.
  6. Penempatan layout slot via `LayoutAllocator`.
  7. Landing perubahan ke base branch via `WorktreeManager::land_worktree`.
  8. Pembersihan worktree via `WorktreeManager::clean_worktree`.

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
  git add crates/temp-terminal-rig/tests/phase2_integration_tests.rs
  git commit -m "test(rig): add phase 2 end-to-end integration test and finalize validation"
  ```
