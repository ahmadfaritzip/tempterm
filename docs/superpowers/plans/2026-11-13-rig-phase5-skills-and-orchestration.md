# Implementation Plan: Rig Phase 5 - Skills Deployment & Cross-Repo Orchestration

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun subsistem penyebaran skill agen (`rig skill`, `rig install`), database hub lintas repositori (`~/.rig/hub.db`), manajemen story koordinasi multi-repo (`rig story`, `rig repos`), serta antarmuka orkestrator (`rig inbox --orch`, `rig board --orch`, `rig launch <repo>`).

**Architecture:** Memperluas `temp-terminal-rig` untuk mendukung operasional multi-agen lintas repositori:
1. `crates/temp-terminal-rig/src/skills.rs`: Menyimpan template instruksi 4 peran kanonikal (*manager*, *worker*, *reviewer*, *orchestrator*) dan menyediakan modul instalasi yang mendeteksi direktori config harness lokal (`claude`, `codex`, `opencode`, `pi`, `omp`, `commandcode`).
2. `crates/temp-terminal-rig/src/hub.rs` & `db.rs`: Menyediakan API CRUD pada database SQLite sentral (`hub.db`) untuk registrasi repositori, story tracking (`Story`, `StoryRepo`), dan pesan broadcast/event orkestrator.
3. `crates/temp-terminal-rig/src/orchestrator.rs`: Mengagregasikan tiket dari seluruh repositori terdaftar yang terikat pada sebuah story dan merender tampilan multi-repo board terpadu (ANSI streaming & JSON).
4. CLI Integration di `src/bin/rig.rs`: Menyediakan subperintah `rig skill`, `rig install`, `rig repos`, `rig story`, `rig launch <path>`, serta opsi `--orch` pada `inbox` dan `board`.

**Tech Stack:** Rust 2021, `rusqlite` (bundled), standard filesystem/process APIs, serde/serde_json, ANSI terminal formatting.

---

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh dimodifikasi sama sekali.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-rig/`.
- DILARANG menggunakan karakter em dash (\u2014) di seluruh antarmuka, komentar kode, dan dokumentasi (Anti-Slop R-02). Gunakan tanda pisah `-`, titik dua `:`, atau titik tengah `·`.
- DILARANG menggunakan literal emoji sebagai dekorasi UI atau log teks (Anti-Slop R-04).
- Keamanan direktori: Operasi instalasi skill hanya boleh menulis ke direktori harness yang terdeteksi secara valid atau path spesifik yang diuji.
- Seluruh tes harus menggunakan repositori sementara (`tempfile::tempdir()`) agar terisolasi dan tidak mencemari direktori pengguna riil (`~/.rig/`).

---

### Task 1: Embedded Skills & CLI Deployment (`skills.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/skills.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/skills_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum RigRole {
      Manager,
      Worker,
      Reviewer,
      Orchestrator,
  }

  impl RigRole {
      pub fn from_str(s: &str) -> Option<Self>;
      pub fn as_str(&self) -> &'static str;
      pub fn prompt_template(&self) -> &'static str;
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct InstalledSkillResult {
      pub harness: String,
      pub skill_dir: std::path::PathBuf,
      pub role: RigRole,
  }

  pub struct SkillsDeployer;

  impl SkillsDeployer {
      pub fn get_skill_dirs(home_dir: &std::path::Path) -> Vec<(&'static str, std::path::PathBuf)>;
      pub fn install_skills(
          home_dir: &std::path::Path,
          target_harnesses: Option<&[&str]>,
      ) -> Result<Vec<InstalledSkillResult>, RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for SkillsDeployer and RigRole**
  Buat `crates/temp-terminal-rig/tests/skills_tests.rs`:
  - `RigRole::from_str` mengenali `"manager"`, `"worker"`, `"reviewer"`, `"orchestrator"`.
  - `RigRole::prompt_template` mengembalikan prompt Markdown yang tidak kosong, tanpa em dash, dan tanpa dekoratif emoji.
  - `SkillsDeployer::install_skills` membuat direktori `rig-<role>/SKILL.md` dengan pointer yang benar pada direktori home tiruan.
  - Pemilihan filter `--harness` membatasi harness mana yang diinstal.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test skills_tests`
  Ekspektasi: Gagal kompilasi karena modul `skills` belum ada.

- [ ] **Step 3: Implement SkillsDeployer and RigRole in `skills.rs`**
  - Definisikan teks prompt statis untuk 4 peran (*manager*, *worker*, *reviewer*, *orchestrator*).
  - Implementasikan pemetaan direktori harness:
    - `claude` -> `.claude/skills`
    - `codex` -> `.codex/skills`
    - `opencode` -> `.config/opencode/skills`
    - `pi` -> `.pi/agent/skills`
    - `omp` -> `.omp/agent/skills`
    - `commandcode` -> `.commandcode/skills`
  - Implementasikan `install_skills`:
    - Membuat folder `rig-{role}`
    - Menulis berkas `SKILL.md` dengan frontmatter YAML dan petunjuk pointer ke `rig skill {role}`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test skills_tests`
  Ekspektasi: Lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/skills.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/skills_tests.rs
  git commit -m "feat(rig): implement embedded skills generator and harness deployer"
  ```

---

### Task 2: Hub Database Operations & Story Management (`hub.rs`, `db.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/hub.rs`
- Modify: `crates/temp-terminal-rig/src/db.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Create: `crates/temp-terminal-rig/tests/hub_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct RegisteredRepo {
      pub name: String,
      pub path: String,
  }

  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Story {
      pub id: String,
      pub title: String,
      pub body: String,
      pub status: String,
      pub created: String,
  }

  impl RigDatabase {
      pub fn register_repo(&self, name: &str, path: &Path) -> Result<(), RigError>;
      pub fn list_repos(&self) -> Result<Vec<RegisteredRepo>, RigError>;
      pub fn remove_repo(&self, name_or_path: &str) -> Result<bool, RigError>;

      pub fn upsert_story(&self, id: &str, title: &str, body: &str) -> Result<Story, RigError>;
      pub fn get_story(&self, id: &str) -> Result<Option<Story>, RigError>;
      pub fn list_stories(&self) -> Result<Vec<Story>, RigError>;
      pub fn link_story_repo(&self, story_id: &str, repo_name: &str) -> Result<(), RigError>;
      pub fn get_story_repos(&self, story_id: &str) -> Result<Vec<String>, RigError>;

      pub fn send_hub_message(
          &self,
          ticket: Option<&str>,
          frm: &str,
          too: &str,
          body: &str,
          story: Option<&str>,
      ) -> Result<Message, RigError>;
      pub fn read_hub_inbox(
          &self,
          too: &str,
          mark_seen: bool,
          story: Option<&str>,
      ) -> Result<Vec<Message>, RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for Hub and Story operations**
  Buat `crates/temp-terminal-rig/tests/hub_tests.rs`:
  - Registrasi repositori ke `hub.db`, pengecekan duplikasi, dan pencabutan (*remove*).
  - Pembuatan story (`upsert_story`), update deskripsi, dan linking repositori ke story.
  - Pengiriman pesan hub dan pembacaan inbox dengan filter story.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test hub_tests`
  Ekspektasi: Gagal kompilasi karena method hub belum diimplementasikan.

- [ ] **Step 3: Implement Hub and Story methods in `db.rs` & `hub.rs`**
  - Implementasikan query SQL pada tabel `repos`, `stories`, `story_repos`, dan `messages` di `db.rs`.
  - Pasang helper di `hub.rs` untuk resolusi direktori hub sentral (`resolve_hub_dir`).

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test hub_tests`
  Ekspektasi: Lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/hub.rs crates/temp-terminal-rig/src/db.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/tests/hub_tests.rs
  git commit -m "feat(rig): implement cross-repo hub database and story tracking"
  ```

---

### Task 3: Orchestrator CLI Subcommands (`skill`, `install`, `repos`, `story`)

**Files:**
- Modify: `crates/temp-terminal-rig/src/bin/rig.rs`
- Modify: `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`

**Interfaces:**
- Ekstensi CLI subcommands:
  - `rig skill [manager|worker|reviewer|orchestrator]`
  - `rig install [harnesses...]`
  - `rig repos [rm <repo>]`
  - `rig story <id> [title]`
  - `rig inbox --orch [--wait] [--story <id>]`

- [ ] **Step 1: Write integration tests for new CLI subcommands**
  Tambahkan pengujian di `crates/temp-terminal-rig/tests/e2e_cli_tests.rs`:
  - `rig skill manager` mencetak prompt template ke stdout.
  - `rig install` menempatkan skill ke direktori harness sementara.
  - `rig repos` dan `rig repos rm` mengelola daftar repositori hub.
  - `rig story S-1 "My Story"` membuat dan menampilkan data story.
  - `rig inbox --orch` membaca pesan dari `hub.db`.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Gagal mengenali subperintah baru.

- [ ] **Step 3: Implement subcommands in `src/bin/rig.rs`**
  - Pasang `cmd_skill(args)`: cetak prompt per peran.
  - Pasang `cmd_install(args)`: deploy skill ke harness.
  - Pasang `cmd_repos(args)`: list / remove repo di hub.
  - Pasang `cmd_story(args)`: create / list stories di hub.
  - Perluas `cmd_inbox(args)`: dukung flag `--orch` dan `--story <id>` yang membuka `hub.db`.
  - Perbarui bantuan pada `print_help()`.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test e2e_cli_tests`
  Ekspektasi: Seluruh pengujian CLI lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/bin/rig.rs crates/temp-terminal-rig/tests/e2e_cli_tests.rs
  git commit -m "feat(rig): wire up skill, install, repos, story, and orch inbox in rig cli"
  ```

---

### Task 4: Multi-Repo Orchestrator Board & Launch (`orchestrator.rs`)

**Files:**
- Create: `crates/temp-terminal-rig/src/orchestrator.rs`
- Modify: `crates/temp-terminal-rig/src/lib.rs`
- Modify: `crates/temp-terminal-rig/src/bin/rig.rs`
- Create: `crates/temp-terminal-rig/tests/orchestrator_tests.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct MultiRepoBoardItem {
      pub repo_name: String,
      pub story_id: Option<String>,
      pub tickets: Vec<Ticket>,
  }

  pub struct OrchestratorEngine;

  impl OrchestratorEngine {
      pub fn aggregate_board(
          hub_db: &RigDatabase,
          story_filter: Option<&str>,
          include_done: bool,
      ) -> Result<Vec<MultiRepoBoardItem>, RigError>;

      pub fn render_orchestrator_board(
          hub_db: &RigDatabase,
          story_filter: Option<&str>,
          include_done: bool,
      ) -> Result<String, RigError>;

      pub fn prepare_launch_manager(
          repo_path: &Path,
          brief_path: &Path,
          harness: Option<&str>,
          model: Option<&str>,
          effort: Option<&str>,
      ) -> Result<LaunchPlan, RigError>;
  }
  ```

- [ ] **Step 1: Write failing tests for OrchestratorEngine**
  Buat `crates/temp-terminal-rig/tests/orchestrator_tests.rs`:
  - Agregasi tiket dari beberapa repositori berbeda berdasarkan registrasi `hub.db`.
  - Filter agregasi berdasarkan ID story (`--story S-1`).
  - Rendering output TUI board multi-repo dengan label repositori dan pengelompokan story.
  - Persiapan launch manager dengan parsing brief dan resolusi harness.

- [ ] **Step 2: Run test to verify it fails**
  Jalankan `cargo test -p temp-terminal-rig --test orchestrator_tests`
  Ekspektasi: Gagal kompilasi karena modul `orchestrator` belum ada.

- [ ] **Step 3: Implement OrchestratorEngine and CLI support in `orchestrator.rs` & `rig.rs`**
  - Implementasikan `aggregate_board`: membuka database repo masing-masing dan mengumpulkan tiket aktif.
  - Implementasikan `render_orchestrator_board`: merender tampilan berjenjang Story -> Repo -> Group -> Ticket.
  - Pasang opsi `rig board --orch [--story S] [--json]` di `src/bin/rig.rs`.
  - Pasang `rig launch <repo_path> --brief <file>` untuk inisialisasi manager di repositori target.

- [ ] **Step 4: Run test to verify it passes**
  Jalankan `cargo test -p temp-terminal-rig --test orchestrator_tests`
  Ekspektasi: Seluruh pengujian lulus 100%.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/temp-terminal-rig/src/orchestrator.rs crates/temp-terminal-rig/src/lib.rs crates/temp-terminal-rig/src/bin/rig.rs crates/temp-terminal-rig/tests/orchestrator_tests.rs
  git commit -m "feat(rig): implement multi-repo orchestrator board aggregation and brief launch"
  ```

---

### Task 5: End-to-End Integration Verification & Anti-Slop Audit

**Files:**
- Create: `crates/temp-terminal-rig/tests/phase5_integration_tests.rs`
- Run: Full test suite & workspace checks.

- [ ] **Step 1: Write full lifecycle Phase 5 integration test**
  Uji alur lengkap orkestrasi lintas repositori:
  1. Inisialisasi hub sentral sementara (`hub.db`).
  2. Inisialisasi 2 repositori git independen (`repo-api` dan `repo-web`).
  3. Registrasi kedua repositori ke hub dan tautkan ke story `S-10: Unified Auth`.
  4. Deploy skill ke folder konfigurasi agen tiruan melalui `SkillsDeployer`.
  5. Buat tiket di kedua repositori dengan tag story `S-10`.
  6. Kirim pesan dari tiket repo ke orchestrator inbox via hub database.
  7. Ambil pesan di orchestrator inbox (`read_hub_inbox`).
  8. Render `rig board --orch` dan pastikan kedua repo muncul di bawah story `S-10`.
  9. Eksekusi pengujian audit Anti-Slop otomatis (0 em dash, 0 emoji).

- [ ] **Step 2: Run all tests in workspace**
  Jalankan:
  ```bash
  cargo test -p temp-terminal-rig
  cargo test -p temp-terminal-ui
  cargo check --workspace --all-targets
  ```
  Ekspektasi: Semua pengujian lulus, nol error, nol warning.

- [ ] **Step 3: Verify Anti-Slop Compliance**
  Lakukan audit otomatis di semua file Phase 5:
  - 0 karakter em dash (\u2014).
  - 0 emoji dekoratif.

- [ ] **Step 4: Commit**
  ```bash
  git add crates/temp-terminal-rig/tests/phase5_integration_tests.rs
  git commit -m "test(rig): add phase 5 end-to-end integration test and finalize validation"
  ```
