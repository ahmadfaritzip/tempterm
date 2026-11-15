# Implementation Plan: Rig Phase 4 - Rig Board TUI, Native GUI Widget & Manager Workflow

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun antarmuka Kanban Board TUI streaming untuk terminal (`tui.rs`), engine triase perubahan kode otomatis bagi Manager (`triage.rs`), widget panel GUI native `RigBoardPanel` di `temp-terminal-ui`, serta subperintah CLI `rig board` dan `rig triage`.

**Architecture:** Memperluas ekosistem `temp-terminal` dengan antarmuka visual terpadu:
1. `crates/temp-terminal-rig/src/tui.rs`: Renderer TUI terminal murni dengan ANSI escape codes, menampilkan 6 kolom/kelompok Kanban (*Needs you*, *Working*, *Ready*, *Queued*, *To do*, *Done*), animasi spinner braille 10-frame (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`), kartu tiket berformat rapi, effort gauge (`❚❚❘❘`), dan drawer activity stream pesan antar-agen.
2. `crates/temp-terminal-rig/src/triage.rs`: Engine triase Manager yang mengekstrak `git diff --stat` dari worktree tiket terhadap base branch, menghitung perubahan baris/file, dan mengevaluasi ambang batas (*threshold*) untuk merekomendasikan atau menugaskan Reviewer independen.
3. `crates/temp-terminal-ui/src/rig_board.rs`: Panel widget egui native `RigBoardPanel` di `temp-terminal-ui` dengan token desain tema terpusat, kartu tiket interaktif ber-radius 6px, tombol aksi kontekstual ("Answer", "Review", "Steer", "Open Pane"), dan pemancar alert toast pada status `needs-you` atau `blocked`.
4. CLI Integration: Subperintah `rig board` (dengan mode `--watch`) dan `rig triage`.

**Tech Stack:** Rust 2021, `rusqlite` (bundled), `egui` 0.30.0, ANSI terminal formatting, standard process/regex APIs.

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh dimodifikasi.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-rig/` dan `temp-terminal/crates/temp-terminal-ui/`.
- DILARANG menggunakan karakter em dash (\u2014) di seluruh antarmuka, komentar kode, dan dokumentasi (Anti-Slop R-02). Gunakan tanda pisah `-`, titik dua `:`, atau titik tengah `·`.
- DILARANG menggunakan literal emoji sebagai dekorasi UI atau log teks (Anti-Slop R-04).
- Desain UI Konsisten: Seluruh styling warna pada GUI wajib menggunakan token terpusat (`crate::theme::*`), dilarang hardcoded RGB raw.
- Kerapian TUI: Renderer ANSI wajib bersih, bebas flickering pada mode streaming watch, dan mendukung output `--json`.

---

### Task 1: ANSI Streaming TUI Board Renderer (`tui.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/tui.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/tui_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct BoardRenderOptions {
      pub show_done: bool,
      pub show_activity: bool,
      pub max_activity_items: usize,
      pub spinner_tick: usize,
  }

  impl Default for BoardRenderOptions {
      fn default() -> Self {
          Self {
              show_done: false,
              show_activity: true,
              max_activity_items: 10,
              spinner_tick: 0,
          }
      }
  }

  pub struct BoardTuiRenderer;

  impl BoardTuiRenderer {
      pub const SPINNER_FRAMES: [&'static str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
      pub fn render_board(db: &mut RigDatabase, options: &BoardRenderOptions) -> Result<String, RigError>;
      pub fn render_effort_gauge(effort: u32) -> &'static str;
  }
  ```

- [ ] **Step 1: Write failing tests for BoardTuiRenderer**
  Buat `crates/temp-terminal-rig/tests/tui_tests.rs` yang menguji:
  - Render 6 kelompok: Needs you, Working, Ready, Queued, To do, Done.
  - Warna ANSI kelompok (merah, amber, hijau, faint, muted, dimmed).
  - Animasi spinner frame pada kelompok Working (`SPINNER_FRAMES[tick % 10]`).
  - Render kartu tiket: ID bold, judul, branch `⎇ rig/T-n`, badge harness, effort gauge.
  - Render activity stream: 10 pesan terakhir dengan format `HH:MM · [T-n] From -> To: Body`.
  - Serialisasi JSON board state.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test tui_tests`
  Ekspektasi: Gagal kompilasi karena modul `tui` belum ada.

- [ ] **Step 3: Implement BoardTuiRenderer in `tui.rs`**
  - Implementasikan `render_effort_gauge`: mengembalikan `❚❘❘❘`, `❚❚❘❘`, `❚❚❚❘`, atau `❚❚❚❚`.
  - Implementasikan `render_board`:
    1. Ambil daftar tiket via `db.list_tickets`.
    2. Kelompokkan tiket ke 6 kategori menggunakan `ticket.group_of(db)`.
    3. Render header dengan nama repo dan waktu.
    4. Render setiap kelompok dengan header kelompok, counter jumlah tiket, dan warna ANSI.
    5. Render kartu tiket berindentasi.
    6. Render bagian bawah (Activity Stream) jika `show_activity` aktif.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test tui_tests`
  Ekspektasi: Seluruh pengujian renderer TUI lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/tui.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/tui_tests.rs
  git commit -m "feat(rig): implement ansi streaming tui board renderer and effort gauge"
  ```

---

### Task 2: Manager Triage & Reviewer Assignment Engine (`triage.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/triage.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/triage_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct DiffStat {
      pub files_changed: usize,
      pub insertions: usize,
      pub deletions: usize,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum TriageAction {
      ApproveLocal,
      AssignReviewer {
          harness: String,
          reason: String,
      },
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct TriageResult {
      pub ticket_id: String,
      pub branch: String,
      pub base: String,
      pub diff_stat: DiffStat,
      pub action: TriageAction,
  }

  pub struct TriageEngine;

  impl TriageEngine {
      pub fn inspect_diff_stat(repo_root: &std::path::Path, ticket: &Ticket) -> Result<DiffStat, RigError>;
      pub fn evaluate_triage(
          repo_root: &std::path::Path,
          db: &mut RigDatabase,
          ticket_id: &str,
          line_threshold: usize,
          reviewer_harness: Option<&str>,
      ) -> Result<TriageResult, RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for TriageEngine**
  Buat `crates/temp-terminal-rig/tests/triage_tests.rs` yang menguji:
  - Parsing output `git diff --stat` atau `--shortstat` menjadi `DiffStat { files_changed, insertions, deletions }`.
  - Evaluasi perubahan kecil (misal 1 file, 10 baris) menghasilkan `TriageAction::ApproveLocal`.
  - Evaluasi perubahan besar (> ambang batas, misal 60 baris atau > 3 file) menghasilkan `TriageAction::AssignReviewer`.
  - Pemilihan harness reviewer dari database settings `reviewer.harness` (default `codex`).

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test triage_tests`
  Ekspektasi: Gagal kompilasi karena modul `triage` belum ada.

- [ ] **Step 3: Implement TriageEngine in `triage.rs`**
  - Implementasikan `inspect_diff_stat`:
    1. Jalankan `git merge-base base branch`.
    2. Jalankan `git diff --shortstat merge-base branch`.
    3. Parsing regex angka "X files changed", "Y insertions", "Z deletions".
  - Implementasikan `evaluate_triage`:
    1. Ambil tiket dari database.
    2. Panggil `inspect_diff_stat`.
    3. Jika `insertions + deletions > line_threshold || files_changed > 3`:
       - Tentukan harness reviewer (argumen atau setting `reviewer.harness`).
       - Kembalikan `AssignReviewer`.
    4. Lainnya, kembalikan `ApproveLocal`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test triage_tests`
  Ekspektasi: Seluruh pengujian triage engine lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/triage.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/triage_tests.rs
  git commit -m "feat(rig): implement manager triage engine and diffstat reviewer assignment"
  ```

---

### Task 3: CLI Subcommands for Board & Triage (`rig board`, `rig triage`)

**Files:**
- Modify: `crates/temp-terminal-rig/src/bin/rig.rs`
- Modify: `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`

**Interfaces:**
- Ekstensi subperintah CLI:
  - `rig board [--watch] [--interval <ms>] [--done] [--json]`
  - `rig triage <ticket_id> [--threshold <lines>] [--reviewer <harness>] [--json]`

- [ ] **Step 1: Write integration tests for new CLI subcommands**
  Tambahkan pengujian di `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`:
  - `rig board` menghasilkan output ANSI yang memuat kelompok dan kartu tiket.
  - `rig board --json` menghasilkan JSON terstruktur.
  - `rig triage <ticket_id>` mengevaluasi diff tiket dan mencetak rekomendasi aksi.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Gagal mengenali subperintah `board` dan `triage`.

- [ ] **Step 3: Implement new subcommands in `src/bin/rig.rs`**
  - Pasang handler argument parsing untuk `board`.
  - Pasang handler argument parsing untuk `triage`.
  - Jika flag `--watch` diberikan pada `board`, jalankan loop pembaruan layar berkala (`thread::sleep`).
  - Perbarui dokumentasi bantuan pada `print_help()`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Seluruh pengujian CLI lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/bin/rig.rs crates/temp-terminal-rig/tests/e2e_cli_tests.rs
  git commit -m "feat(rig): wire up board and triage commands in rig cli"
  ```

---

### Task 4: Native Egui RigBoardPanel Widget (`crates/temp-terminal-ui`)

**Files:**
- Modify: `crates/temp-terminal-ui/Cargo.toml` (tambahkan dependency `temp-terminal-rig`)
- Create: `crates/temp-terminal-ui/src/rig_board.rs`
- Modify: `crates/temp-terminal-ui/src/lib.rs`
- Modify: `crates/temp-terminal-ui/src/workspace.rs`
- Create: `crates/temp-terminal-ui/tests/rig_board_ui_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone)]
  pub struct RigBoardState {
      pub repo_path: std::path::PathBuf,
      pub show_done: bool,
      pub expanded_groups: [bool; 6], // NeedsYou, Working, Ready, Queued, Todo, Done
      pub selected_ticket: Option<String>,
      pub pending_toasts: Vec<String>,
  }

  pub struct RigBoardPanel;

  impl RigBoardPanel {
      pub fn ui(ui: &mut egui::Ui, state: &mut RigBoardState, db: &mut RigDatabase);
  }
  ```

- [ ] **Step 1: Write tests for RigBoardState**
  Buat `crates/temp-terminal-ui/tests/rig_board_ui_tests.rs` yang menguji:
  - Inisialisasi `RigBoardState` dengan default expanded groups (Needs you, Working, Ready aktif; Done kolaps).
  - Penambahan notifikasi toast saat ada tiket berstatus Needs you atau Blocked.
  - Seleksi kartu tiket.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-ui --test rig_board_ui_tests`
  Ekspektasi: Gagal kompilasi karena modul `rig_board` belum ada.

- [ ] **Step 3: Implement RigBoardPanel in `crates/temp-terminal-ui/src/rig_board.rs`**
  - Implementasikan `RigBoardState` dan `RigBoardPanel::ui`.
  - Gunakan token warna tema `temp_terminal_ui::workspace::theme`:
    - `STATUS_ERROR` untuk Needs you
    - `STATUS_WARNING` untuk Working
    - `STATUS_OK` untuk Ready
    - `TEXT_MUTED` / `TEXT_FAINT` untuk Queued dan Todo
  - Gambar kartu tiket dengan `SURFACE_CARD`, sudut lengkung 6.0, border halus.
  - Tambahkan tombol aksi: "Answer", "Review", "Steer", "Open Pane".
  - Integrasikan pemicu toast notifikasi saat status kritis terdeteksi.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-ui --test rig_board_ui_tests`
  Ekspektasi: Seluruh pengujian komponen UI lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-ui/Cargo.toml crates/temp-terminal-ui/src/rig_board.rs crates/temp-terminal-ui/src/lib.rs crates/temp-terminal-ui/tests/rig_board_ui_tests.rs
  git commit -m "feat(ui): implement native egui rig board panel and group accordions"
  ```

---

### Task 5: End-to-End Integration Verification & Anti-Slop Audit

**Files:**
- Create: `crates/temp-terminal-rig/tests/phase4_integration_tests.rs`
- Run: Full test suite & workspace checks.

- [ ] **Step 1: Write full lifecycle Phase 4 integration test**
  Uji alur terpadu lengkap:
  1. Pembuatan tiket berbagai status (Needs you, Working, Ready, Queued, To do, Done).
  2. Render ANSI streaming board via `BoardTuiRenderer::render_board`.
  3. Verifikasi ketiadaan flicker, akurasi spinner frames, dan format activity stream.
  4. Simulasi komit perubahan pada worktree tiket.
  5. Evaluasi triase via `TriageEngine::evaluate_triage`.
  6. Penugasan reviewer jika melebihi ambang batas.
  7. Verifikasi konsistensi state UI board.

- [ ] **Step 2: Run all tests in workspace**
  Jalankan:
  ```bash
  cargo test -p temp-terminal-rig
  cargo test -p temp-terminal-ui --test rig_board_ui_tests
  cargo check --workspace --all-targets
  ```
  Ekspektasi: Semua pengujian lulus, nol error, nol warning.

- [ ] **Step 3: Verify Anti-Slop Compliance**
  Lakukan audit otomatis:
  - Nol karakter em dash (\u2014) di seluruh kode baru.
  - Nol emoji dekoratif.
  - Nol warna raw RGB hardcoded di GUI (semua token terpusat).

- [ ] **Step 4: Commit**
  ```bash
  git add crates/temp-terminal-rig/tests/phase4_integration_tests.rs
  git commit -m "test(rig): add phase 4 end-to-end integration test and finalize validation"
  ```
