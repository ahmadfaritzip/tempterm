# Recon Map: herdr-rig (CLI / TUI / Desktop App Mod)

Scope: Multi-Agent Orchestration & Management Engine for Terminal Workspaces
For: `temp-terminal` (Rust Native Terminal Workspace Integration)
Date: 2026-11-12

---

## 1. Sources

| # | Source | Location / URL | Notes |
|---|---|---|---|
| 1 | Architecture & Core CLI | `../herdr-rig/rig.ts` | 1,193 baris kode TypeScript murni: CLI, SQLite, Multiplexer abstraction, Watcher, TUI Board |
| 2 | Test Suite & Invariants | `../herdr-rig/rig.test.ts` | 313 baris pengujian end-to-end: worktrees, squash merges, queueing, harness expansion |
| 3 | Manager Skill Runbook | `../herdr-rig/skills/manager/SKILL.md` | Protokol interaksi human-to-manager, ticket drafting, dispatching, review triage, autonomy |
| 4 | Orchestrator Skill | `../herdr-rig/skills/orchestrator/SKILL.md` | Koordinasi lintas-repositori (cross-repo stories, hub registry, shared contracts) |
| 5 | Worker Skill Runbook | `../herdr-rig/skills/worker/SKILL.md` | Aturan kerja agen pelaksana: worktree isolation, milestone reporting, verify step |
| 6 | Reviewer Skill Runbook | `../herdr-rig/skills/reviewer/SKILL.md` | Aturan verifikasi diff independen: evaluasi acceptance criteria, PASS/FIX verdict |
| 7 | Desktop App GUI Mod | `../herdr-rig/mods/rig-board/main.js` | Komponen visual GPUI 332 baris: auto-refresh, toast alerts, card interaction, live status |
| 8 | Dokumentasi & Changelog | `../herdr-rig/README.md`, `CHANGELOG.md` | Filosofi desain, harness support matrix, layout tile algorithm, roadmap |

---

## 2. Core Loop

Satu percakapan terpadu dengan agen **Manager** yang memecah instruksi pengguna menjadi tiket terisolasi (git worktree), mendelegasikan eksekusi ke agen **Worker** (Claude Code, Codex, OpenCode, Pi, OMP, Command Code), memverifikasi hasil secara independen dengan agen **Reviewer**, mengawasi status agen secara otomatis tanpa polling (*auto-watcher*), dan menampilkan seluruh progres pada papan kanban (*Board*) terintegrasi.

---

## 3. Screen Inventory

| ID | Screen / View | Route / How to Reach | Purpose | Key Components | States Seen |
|---|---|---|---|---|---|
| S01 | TUI Board (Terminal Live View) | `rig board` (atau auto-open via manager) | Menampilkan status tiket dan aktivitas agen secara real-time di terminal | Header, Status Counter, Group Columns, Ticket Row, Assignee Badge, Live Spinner, Activity Feed | Empty, Active, Filtered (`--mine`), Once |
| S02 | TUI Orchestrator Board | `rig board --orch [--story S-n]` | Monitoring tiket lintas-repo dikelompokkan per Story | Story Separator, Repo Header, Cross-repo Tickets, Activity Stream | Empty, Multi-repo, Filtered per story |
| S03 | GUI Board Panel (App Mod) | `rig board --open` / Panel samping GUI | Panel kanban visual native pada terminal workspace | Group Accordion, Card List, Status Mark, Effort Meter, Assignee Chip, Action Buttons ("Open", "Answer", "Review") | Empty, Collapsed, Expanded, Alert Toast |
| S04 | Manager Interactive Session | Pane utama kiri (50% lebar layar) | Sesi dialog pengguna dengan Manager untuk perencanaan & pengawasan | Prompt area, ticket preview, unblock notice, inbox summary | Idle, Planning, Awaiting Input, Reviewing |
| S05 | Worker Live Terminal Pane | Pane kanan bawah / Dedicated tab / Worktree space | Eksekusi tugas tangan-pertama oleh agen AI pekerja di worktree terisolasi | Shell session, PTY output, git worktree status, milestone logs | Starting, Working (spinning), Idle, Blocked, Exited |
| S06 | Reviewer Live Terminal Pane | Split kanan di samping worker | Sesi verifikasi read-only untuk memeriksa diff dan menjalankan command verifikasi | Diff viewer, verify terminal, verdict prompt | Active, Pass, Fix, Closed |
| S07 | Interactive Setup Wizard | Terminal prompt saat pertama kali run | Mengonfigurasi default harness, model, mode, worktree, autonomy, merge | Numbered list / Interactive choice selector | First run, Saved |
| S08 | Ticket Detail View | `rig show <T-n>` | Inspeksi lengkap tiket: goal, acceptance, verify, messages, runs | Markdown text dump, message thread, runs history table | Open, Blocked, Review, Done |
| S09 | Process / Run Inspector | `rig ps [--all]` | Daftar eksekusi agen yang sedang aktif dan status PID/pane | Table: ID, Role, Harness, Model, Mode, Status, Pane | Empty, Running, Exited |
| S10 | Message Inbox Stream | `rig inbox [--wait]` | Aliran pesan masuk antar-agen dengan listener hening di background | Message log: Timestamp, From, Ticket ID, Body | Empty, Unread, Waiting |

---

## 4. User Flows

### F01: Initial Setup in Repo (Konfigurasi Satu Kali)
- Jalur: Masuk ke folder git -> Jalankan manager (`/rig-manager` atau `rig setup`) -> Sistem mendeteksi `rig get setup` kosong -> Tampilkan 6 pertanyaan terpadu:
  1. `worker.harness / model / effort` (agen pembuat kode)
  2. `reviewer.harness / model / effort` (agen reviewer, beda keluarga model)
  3. `mode` (`pane` / `tab` / `headless`)
  4. `worktree` (`on` / `off` / `herdr`)
  5. `autonomy` (`review` / `ask` / `merge`)
  6. `merge` (`local` / `pr`)
- Simpan ke `.rig/rig.db` (`settings` table) -> Tandai `setup=done`.
- Total klik/interaksi happy path: 1 respons pilihan jamak.

### F02: Ticket Creation & Dependency Queuing (Perumusan Tugas)
- Jalur: S04 Manager mendengarkan instruksi pengguna -> Membedah menjadi tiket terstruktur:
  `rig new "Judul Perintah" --body "Goal: ... Acceptance: ... Verify: ... Scope: ..." --base <branch> [--after T-1,T-2]`
- Sistem mengalokasikan ID inkremental `T-1`, `T-2`, dst.
- Jika ada parameter `--after`, status tiket disetel ke `Queued`.
- Tiket langsung muncul di S01/S03 pada grup "To do" atau "Queued".

### F03: Dispatching Worker Run (Delegasi ke Agen Pelaksana)
- Jalur: S04 Manager mengeksekusi `rig dispatch T-1 [note] [--harness ..] [--model ..] [--mode ..]`:
  1. Evaluasi `worktreePlan`: buat worktree `.rig/wt/T-1` pada branch `rig/T-1` dari base branch saat ini.
  2. Alokasikan layout terminal via algoritma `slot`:
     - Pane manager tetap di kiri (50%).
     - Board membuka di kanan atas (50%).
     - Worker pane membuka di bawah board (membagi tinggi secara proporsional).
     - Jika jumlah pane melebihi `pane.max` (default 2), limpahkan ke tab overflow (`over-1`, `over-2`) dengan layout 2 kolom (maksimal `tab.max` = 8 pane).
  3. Generate prompt awal dan luncurkan proses harness (`interactive`, `headless`, atau `resume`).
  4. Pasang watcher background (`_watch`) untuk memantau status sesi PTY.
  5. Update status tiket menjadi `working`.

### F04: Active Run Supervision & Agent Watching (Pengawasan Otonom)
- Jalur: Worker berjalan di S05 -> Watcher (`_watch`) menunggu status agen menyelesaikan gilirannya (*turn*):
  - Jika agen menyelesaikan tugas dan mengirim pesan `rig say manager "done: ..."`: Watcher mencatat pesan telah terkirim.
  - Jika agen berhenti (*settled* menjadi `idle`, `blocked`, atau `exited`) tanpa mengirim laporan: Watcher otomatis mengekstrak 12 baris terakhir layar terminal (`tail(screen)`) dan mengirimkan pesan alert ke Manager: `[watch] worker run #1 is idle without reporting. Screen: ...`.
  - Jika tiket berstatus `needs-you` atau `blocked`: Tampilkan notifikasi toast di GUI workspace.

### F05: Inter-Agent Messaging & Quiet Inbox (Komunikasi Antar-Agen Hening)
- Jalur: Agen Worker/Reviewer memanggil `rig say manager "<pesan>"`:
  - Pesan disimpan ke tabel `messages` di `.rig/rig.db`.
  - Jika Manager sedang menjalankan `rig inbox --wait` di background: Listener langsung bangun dan memproses pesan tanpa menginterupsi terminal.
  - Jika Manager tidak memiliki listener aktif dan pengguna sedang mengetik di pane Manager: Pesan ditahan di antrean background (`_deliver`) hingga fokus berpindah, mencegah tabrakan dengan draf ketikan pengguna.
  - Saat fokus berpindah, sistem mendorong gabungan pesan: `[rig] N messages while you were busy: ...`.

### F06: Steering & Interrupting Runs (Intervensi Langsung)
- Jalur: Pengguna atau Manager ingin mengubah arahan agen:
  - Manager mengeksekusi `rig tell T-1 "<arahan baru>" [--now]`.
  - Jika mode `pane`: Kirim sinyal Escape/Interrupt jika `--now` aktif, lalu ketikkan arahan ke terminal worker. Status tiket dikembalikan ke `working`.
  - Jika mode `headless`: Matikan PID lama (`kill`), lalu lanjutkan sesi harness via sub-perintah `resume` dengan pesan arahan baru.

### F07: Review Loop (Verifikasi Kualitas Kode Independen)
- Jalur: Worker menyelesaikan tugas -> Menjalankan verifikasi lokal -> Mengubah status `rig edit T-1 status=review` -> Mengirim laporan ke manager:
  1. Manager menerima notifikasi di S04.
  2. Manager melakukan **Triage Ringkas**: hanya memeriksa `git diff <base>...HEAD --stat` dan ringkasan laporan worker (tidak membaca keseluruhan diff untuk menjaga context window).
  3. Manager memilih satu pembaca kode:
     - Jika perubahan kompleks: Manager memanggil `rig dispatch T-1 --role reviewer`. Agen Reviewer (S06) dijalankan di worktree yang sama menggunakan keluarga model yang berbeda.
     - Jika perubahan sepele: Manager membaca diff sendiri.
  4. Reviewer menguji acceptance criteria dan verify command:
     - Jika ada cacat: Reviewer mengirim `rig say manager "FIX: 1) ... 2) ..."`. Manager meneruskan temuan ke worker via `rig tell T-1`.
     - Jika bersih: Reviewer mengirim `rig say manager "PASS: ..."`.
  5. Selesai review: Manager menutup sesi reviewer via `rig close <run#>`.

### F08: Landing Work & Cleanup (Penyelesaian & Pembersihan)
- Jalur: Tiket berstatus review disetujui (PASS):
  - Jika `autonomy=merge` atau setelah konfirmasi pengguna:
    * Mode `local`: Switch ke base branch -> `git merge --squash rig/T-1` -> `git commit -m "<title> (T-1)"`.
    * Mode `pr`: Push branch `rig/T-1` -> buat PR dengan `gh pr create`.
  - Update tiket: `rig edit T-1 status=done`.
  - Sistem menutup pane/tab yang terkait dengan tiket tersebut (`closeRuns`).
  - Sistem memeriksa apakah seluruh perubahan branch sudah mendarat di base branch (`landed()`): jika sudah, hapus worktree `.rig/wt/T-1` dan hapus branch `rig/T-1`.
  - Jika ada tiket lain yang menunggu tiket ini (`--after T-1`), sistem otomatis mengirim pesan unblock ke Manager.

### F09: Cross-Repo Orchestration (Koordinasi Multi-Repositori)
- Jalur: Pengguna memiliki perubahan yang mencakup beberapa repo (misal: API backend, Web client, dan SDK):
  1. Jalankan Orchestrator (`/rig-orchestrator`).
  2. Orchestrator meminta path repositori dan mendaftarkannya di database sentral `~/.rig/hub.db`.
  3. Orchestrator menyusun brief untuk masing-masing repo dengan shared contract yang identik, order pengerjaan, dan Story ID (misal: `S-12`).
  4. Orchestrator memanggil `rig launch <repo>` untuk menyalakan Manager di setiap repositori.
  5. Orchestrator mendengarkan event via `rig inbox --orch --story S-12 --wait`.
  6. Papan S02 memantau seluruh repo di bawah payung Story `S-12`.

---

## 5. UI Components

| Component | Variants | States | Used On |
|---|---|---|---|
| Board Window / Panel | TUI (terminal), GUI Pane (docked), Floating Overlay | Normal, Minimized, Fullscreen | S01, S02, S03 |
| Group Column / Section | Needs you, Working, Ready, Queued, To do, Done | Expanded, Collapsed, Empty | S01, S02, S03 |
| Ticket Card | Default, Focused, Orphan (working without live process) | Normal, Hover, Active, Blocked | S01, S03 |
| Status Indicator Mark | Dot (●, ○, ◆), Spinner (⠋..⠏), Check (✓), Alert (!) | Idle, Working, Blocked, Needs-You, Done | S01, S02, S03 |
| Harness Badge | Claude, Codex, OpenCode, Pi, OMP, CommandCode | Solid, Dim, Outlined | S01, S03, S09 |
| Effort Gauge | 4-bar indicator (minimal: 1, low: 1, medium: 2, high: 3, max: 4) | Inactive, Active level | S01, S03 |
| Action Button | "Open Pane", "Answer", "Review", "Kill", "Clear" | Default, Hover, Pressed, Disabled | S03, S08 |
| Dependency Pill | `waiting on T-1 (open), T-2 (done)` | Queued, Blocked, Resolved | S01, S03, S08 |
| Activity Feed Row | Regular message, Steer event, Watcher alert | Fresh (<1m), Old | S01, S03, S10 |
| Layout Splitter / Slot | Horizontal split, Vertical split, New Tab, Space | Left, Right, Down, Tab, Space | S04, S05, S06 |

---

## 6. Inferred Data Model

### 6.1 Local Repository Database (`<repo>/.rig/rig.db`)

```sql
-- Tickets table: unit of work definition
CREATE TABLE tickets (
    id TEXT PRIMARY KEY,             -- Format: 'T-1', 'T-2', ...
    title TEXT NOT NULL,             -- Imperative action title
    body TEXT,                       -- Markdown brief (Goal, Acceptance, Verify, Scope, Context)
    status TEXT DEFAULT 'open',      -- 'open', 'draft', 'working', 'reviewing', 'review', 'needs-you', 'blocked', 'done'
    manager TEXT,                    -- Identity of the managing pane/session
    review INTEGER DEFAULT 0,        -- Flag (0 or 1): requires independent reviewer
    branch TEXT,                     -- Git branch name (e.g. 'rig/T-1')
    worktree TEXT,                   -- Path to git worktree (e.g. '.rig/wt/T-1')
    created TEXT NOT NULL,           -- ISO-8601 / UTC timestamp
    after TEXT,                      -- Comma/space-separated prerequisite ticket IDs ('T-3,T-4')
    base TEXT,                       -- Source & target branch (e.g. 'main', 'feat-x')
    story TEXT                       -- Optional cross-repo story ID (e.g. 'S-12')
);

-- Runs table: active execution sessions
CREATE TABLE runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket TEXT NOT NULL,            -- Associated ticket ID
    role TEXT NOT NULL,              -- 'worker' or 'reviewer'
    harness TEXT NOT NULL,           -- 'claude', 'codex', 'opencode', 'pi', 'omp', 'commandcode'
    model TEXT,                      -- Model override string
    effort TEXT,                     -- 'low', 'medium', 'high', 'max'
    mode TEXT NOT NULL,              -- 'pane', 'tab', 'headless', 'space', 'over-N'
    pane TEXT,                       -- Multiplexer pane/terminal ID
    sid TEXT,                        -- Unique session UUID for harness resumption
    pid INTEGER,                     -- OS process ID (especially for headless runs)
    cmd TEXT,                        -- Serialized JSON argv executed
    status TEXT NOT NULL,            -- 'starting', 'running', 'stopped', 'exited', 'closed', 'failed'
    cwd TEXT NOT NULL,               -- Working directory (worktree root or repo root)
    started TEXT NOT NULL            -- Timestamp
);

-- Messages table: asynchronous communication log
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ticket TEXT,                     -- Related ticket ID (optional)
    frm TEXT NOT NULL,               -- Sender identity ('manager:p1', 'T-1/worker', 'orchestrator')
    too TEXT NOT NULL,               -- Recipient identity ('manager:p1', 'T-1/worker', 'human')
    body TEXT NOT NULL,              -- Message content
    seen INTEGER DEFAULT 0,          -- Read status flag (0 = unread, 1 = read/delivered)
    at TEXT NOT NULL,                -- Timestamp
    story TEXT                       -- Associated story ID
);

-- Settings table: persistent key-value configuration
CREATE TABLE settings (
    k TEXT PRIMARY KEY,              -- Config key (e.g. 'worker.harness', 'autonomy', 'mode')
    v TEXT                           -- Config value string
);
```

### 6.2 Global Machine Hub Database (`~/.rig/hub.db`)

```sql
-- Repositories registered in the machine hub
CREATE TABLE repos (
    path TEXT PRIMARY KEY,           -- Absolute path to git repository root
    name TEXT NOT NULL,              -- Directory / project name
    manager TEXT,                    -- Active manager session reference
    added TEXT NOT NULL              -- Timestamp
);

-- Cross-repo stories
CREATE TABLE stories (
    id TEXT PRIMARY KEY,             -- E.g. 'S-12'
    title TEXT NOT NULL              -- Short descriptive name
);

-- Link table between stories and repositories
CREATE TABLE story_repos (
    story TEXT NOT NULL,
    path TEXT NOT NULL,
    PRIMARY KEY(story, path)
);

-- Cross-repo message hub
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo TEXT NOT NULL,
    ticket TEXT,
    frm TEXT NOT NULL,
    too TEXT NOT NULL,
    body TEXT NOT NULL,
    seen INTEGER DEFAULT 0,
    at TEXT NOT NULL,
    story TEXT
);

CREATE TABLE settings (
    k TEXT PRIMARY KEY,
    v TEXT
);
```

---

## 7. Feature Matrix Summary

See detailed breakdown in `replica/features.csv`.
- **Must Have (Paritas Inti)**:
  - SQLite database schemas (`rig.db` & `hub.db`) dengan PRAGMA WAL & busy timeout.
  - CRUD Tiket: `new`, `edit`, `ls`, `show`, dependency resolution (`--after`).
  - Git Worktree Engine: pembuatan branch `rig/T-n`, isolasi folder `.rig/wt/T-n`, deteksi `landed()`, squash merge, dan pembersihan otomatis (`rig clean`).
  - Asynchronous Message Bus: tabel pesan, mekanisme `say`, `inbox --wait`, dan antrean hening `_deliver`.
  - Process Launcher & Watcher: peluncuran agen AI dengan argument expansion, monitoring idle/blocked/exited, tail screen capture.
  - TUI Board: tampilan kanban live di terminal dengan pengelompokan status, counter, spinner, dan activity stream.
- **Should Have (Integrasi Native Terminal Workspace)**:
  - GUI Board Panel: integrasi native tab / sidebar board pada UI `temp-terminal` (egui) dengan polling background dan aksi langsung.
  - Auto-layout manager/board/worker: layout split 50/50 dan overflow tabs.
  - Multi-harness CLI template expansion (Claude Code, Codex, OpenCode, Pi, OMP).
  - Cross-repo Orchestrator (`rig launch`, `hubBoard`, story management).
- **Could Have (Penyempurnaan Lanjutan)**:
  - Desktop Toast notifications pada event `needs-you` atau `blocked`.
  - Integrasi PR via `gh` CLI.
  - Visual effort gauge 4-bar pada panel GUI.

---

## 8. Out of Scope (Apa yang Disesuaikan / Tidak Di-clone Mentah)

1. **Ketergantungan Eksternal Bun / Node**: `herdr-rig` asli ditulis dengan TypeScript dan berjalan di atas runtime Bun. Pada `temp-terminal`, seluruh engine ini akan ditulis ulang **100% secara native dalam Rust** (menggunakan `rusqlite` atau SQLite murni, `tokio::process`, dan modul internal).
2. **Ketergantungan Eksternal `herdr` / `hc` CLI**: `herdr-rig` mengandalkan IPC ke binary eksternal `herdr` dan `herdr-app`. Di dalam `temp-terminal`, aplikasi kita **adalah** terminal emulator dan window multiplexer itu sendiri! Kontrol PTY, split pane, dan tab session dapat langsung dipanggil melalui fungsi internal Rust (`SessionManager`, `ConPTY`, `SplitLayout`).
3. **GPUI Mod Framework**: Mod board asli menggunakan JavaScript/GPUI. Pada `temp-terminal`, kita mengimplementasikannya sebagai **Native Egui Widget / Dock Panel** yang menyatu dengan tema dan rendering loop aplikasi.

---

## 9. Sizing & Complexity

- **Skala**:
  - 10 Layar / Tampilan (TUI + GUI + Sub-views).
  - 10 Alur Kerja (User Flows).
  - 4 Tabel Database Lokal + 5 Tabel Database Sentral Hub.
- **Tingkat Kesulitan**:
  - **Medium-High (M-L)**: Membutuhkan integrasi erat antara Rust backend (proses, PTY, Git worktree, SQLite) dan Rust frontend (Egui tiling, status polling, terminal rendering).
- **Langkah Berikutnya**:
  - Menyusun dokumen cetak biru implementasi arsitektur native Rust: [`docs/BLUEPRINT-HERDR-RIG-INTEGRATION.md`](file:///D:/project/myproject/rewrite-terminal/temp-terminal/docs/BLUEPRINT-HERDR-RIG-INTEGRATION.md).
