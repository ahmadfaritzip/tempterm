# Cetak Biru Arsitektur: Integrasi herdr-rig ke dalam temp-terminal

- **Tanggal**: 2026-11-12
- **Status**: Disetujui sebagai Panduan Rekayasa (*Master Blueprint*)
- **Referensi Recon**: [`replica/recon.md`](file:///D:/project/myproject/rewrite-terminal/temp-terminal/replica/recon.md), [`replica/features.csv`](file:///D:/project/myproject/rewrite-terminal/temp-terminal/replica/features.csv)
- **Target Proyek**: `temp-terminal` (Rust Native Terminal Workspace)

---

## 1. Ringkasan Eksekutif & Tujuan

`herdr-rig` adalah sistem orkestrasi multi-agen yang elegan dan berorientasi pada kesederhanaan (*minimal by design*). Alih-alih membuat arsitektur daemon terpusat yang rumit atau MCP server berat, rig menempatkan seluruh pertimbangan (*judgement*) pada empat keterampilan agen AI (**Manager**, **Orchestrator**, **Worker**, **Reviewer**), dengan koordinasi berbasis CLI ringan, database embedded SQLite, isolasi git worktree, dan antarmuka papan (*Board*).

Tujuan integrasi ini adalah memboyong seluruh kapabilitas `herdr-rig` ke dalam `temp-terminal` secara **100% Native Rust**. Karena `temp-terminal` sudah memiliki terminal multiplexer, emulasi ConPTY, engine pendeteksi agen (*agent detection*), dan antarmuka egui modern, integrasi ini akan jauh lebih cepat, stabil, dan terpadu tanpa ketergantungan runtime Bun, NodeJS, maupun binary CLI eksternal.

---

## 2. Perbandingan Arsitektur: herdr-rig vs temp-terminal Native

| Komponen | herdr-rig Asli (TypeScript/Bun) | temp-terminal Integrasi (Rust Native) | Keuntungan Integrasi Native |
|---|---|---|---|
| **Runtime & Bahasa** | Bun >= 1.2, TypeScript | Rust 2021 (Native Binary) | Nol ketergantungan runtime Bun, startup instan (<1ms), jejak memori minimal |
| **Penyimpanan State** | SQLite (`bun:sqlite`), WAL mode | SQLite via `rusqlite` (bundled / native), WAL mode | Thread-safe, compile-time query check, nol overhead FFI dinamis |
| **Terminal Multiplexing** | Mengontrol binary eksternal `herdr` / `hc` via sub-process CLI | Internal `SessionManager`, `ConPTY` stream, dan `Workspace` split layout | Kontrol instan tanpa spawning subproses herdr; bebas latensi IPC shell |
| **Agent Watching** | Polling CLI `herdr agent get` / `hc wait` tiap 2-3 detik | Event-driven via internal PTY buffer reader & `agent_detection.rs` | Deteksi instan saat agen idle/blocked/exited tanpa polling; zero CPU overhead |
| **Papan Monitoring (Board)** | TUI terminal + Mod JavaScript GPUI terpisah | TUI terintegrasi (`crossterm`/`ratatui`) + Egui Widget Native Panel | Panel board menyatu langsung dengan tema aplikasi (`crate::theme::*`), tanpa mod terpisah |
| **Git Worktrees** | Memanggil binary CLI `git worktree` | `git` subprocess teroptimasi / `git2` bindings | Penanganan locking Windows yang aman (*automatic retry & process release sweep*) |
| **Inter-Agent Messaging** | Polling database `readInbox` + antrean background process | Kanal asinkron `tokio::sync::broadcast` + SQLite log persisten | Respons pesan instan tanpa delay 2 detik; penyimpanan riwayat lengkap |

---

## 3. Struktur Modul & Dekomposisi Crate Baru

Kami merekomendasikan penambahan crate baru di dalam workspace cargo: `crates/temp-terminal-rig` (atau sebagai modul terisolasi di `temp-terminal-core`), serta integrasi visual di `temp-terminal-ui`.

```text
temp-terminal/
├── crates/
│   ├── temp-terminal-core/           # Inti protokol, daemon, paths, runner
│   ├── temp-terminal-rig/            # [BARU] Engine Orkestrasi & State Rig
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                # Public API entrypoint
│   │       ├── db.rs                 # Schema migration, connection pooling, PRAGMA WAL
│   │       ├── ticket.rs             # Model Ticket, parsing brief, dependency graph
│   │       ├── run.rs                # Model Run, harness expansion (Claude, Codex, dll.)
│   │       ├── message.rs            # Bus pesan inter-agen, say, quiet delivery
│   │       ├── worktree.rs           # Git worktree lifecycle, merge-base diff, cleanup
│   │       ├── watcher.rs            # Turn inspector, unprompted settle detector
│   │       ├── layout.rs             # Algoritma slot alokasi pane/tab 2-kolom
│   │       ├── hub.rs                # Hub SQLite ~/.rig/hub.db & cross-repo stories
│   │       └── board_data.rs         # Serialisasi agregasi board snapshot
│   ├── temp-terminal-ui/             # Antarmuka Grafis (Egui)
│   │   └── src/
│   │       ├── rig_board_panel.rs    # [BARU] Widget panel board grafis native
│   │       ├── agent_detection.rs    # Terhubung dengan status agen rig
│   │       └── workspace.rs          # Integrasi docking panel board & split layout
│   └── temp-terminal-cli/            # CLI Tools
│       └── src/
│           └── rig_cmd.rs            # Sub-perintah `temp-terminal rig <cmd>`
└── skills/                           # Keterampilan Agen
    ├── rig-manager/SKILL.md          # Instruksi peran Manager
    ├── rig-orchestrator/SKILL.md     # Instruksi peran Orchestrator
    ├── rig-worker/SKILL.md           # Instruksi peran Worker
    └── rig-reviewer/SKILL.md         # Instruksi peran Reviewer
```

---

## 4. Rincian Desain Arsitektur Rust

### 4.1 Skema Database & Lapisan Akses Data (`db.rs`)
Menggunakan `rusqlite` dengan WAL mode dan timeout busy 5000ms:

```rust
pub struct RigDatabase {
    conn: rusqlite::Connection,
}

impl RigDatabase {
    pub fn open_repo(repo_root: &Path) -> Result<Self, RigError> {
        let rig_dir = repo_root.join(".rig");
        std::fs::create_dir_all(&rig_dir)?;
        let db_path = rig_dir.join("rig.db");
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch("
            PRAGMA busy_timeout = 5000;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS tickets (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                body TEXT,
                status TEXT DEFAULT 'open',
                manager TEXT,
                review INTEGER DEFAULT 0,
                branch TEXT,
                worktree TEXT,
                created TEXT NOT NULL,
                after TEXT,
                base TEXT,
                story TEXT
            );
            CREATE TABLE IF NOT EXISTS runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ticket TEXT NOT NULL,
                role TEXT NOT NULL,
                harness TEXT NOT NULL,
                model TEXT,
                effort TEXT,
                mode TEXT NOT NULL,
                pane TEXT,
                sid TEXT,
                pid INTEGER,
                cmd TEXT,
                status TEXT NOT NULL,
                cwd TEXT NOT NULL,
                started TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ticket TEXT,
                frm TEXT NOT NULL,
                too TEXT NOT NULL,
                body TEXT NOT NULL,
                seen INTEGER DEFAULT 0,
                at TEXT NOT NULL,
                story TEXT
            );
            CREATE TABLE IF NOT EXISTS settings (
                k TEXT PRIMARY KEY,
                v TEXT
            );
        ")?;
        Ok(Self { conn })
    }
}
```

### 4.2 Manajemen Siklus Hidup Tiket & Antrean (`ticket.rs`)
Model tiket membedakan 6 status utama sesuai kelompok Board:
- **`Needs you`**: `needs-you`, `blocked`
- **`Working`**: `working`, `reviewing`
- **`Ready`**: `review`
- **`Queued`**: status `open` atau `draft`, tetapi memiliki prasyarat (`after`) yang belum `done`
- **`To do`**: `open`, `draft` tanpa blokir
- **`Done`**: `done`

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

impl TicketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Draft => "draft",
            Self::Working => "working",
            Self::Reviewing => "reviewing",
            Self::Review => "review",
            Self::NeedsYou => "needs-you",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }
}
```

Fungsi evaluasi dependensi:
```rust
pub fn is_queued(db: &RigDatabase, ticket: &Ticket) -> bool {
    if ticket.status != TicketStatus::Open && ticket.status != TicketStatus::Draft {
        return false;
    }
    let unresolved_dependencies = ticket.waiting_on(db);
    !unresolved_dependencies.is_empty()
}
```

### 4.3 Git Worktree Engine & Landing Safety (`worktree.rs`)
Memastikan isolasi kerja yang aman pada branch `rig/T-n` dan verifikasi bahwa perubahan telah mendarat (*landed*) sebelum branch atau folder worktree dihapus:

```rust
pub struct WorktreeManager;

impl WorktreeManager {
    pub fn create_worktree(repo_root: &Path, ticket_id: &str, base_branch: &str) -> Result<PathBuf, RigError> {
        let wt_path = repo_root.join(".rig").join("wt").join(ticket_id);
        let branch_name = format!("rig/{}", ticket_id);
        
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", &branch_name, base_branch])
            .output()?;
            
        if !output.status.success() && !wt_path.exists() {
            return Err(RigError::GitError(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        Ok(wt_path)
    }

    pub fn has_landed(repo_root: &Path, base_branch: &str, ticket_branch: &str) -> bool {
        // Cek merge-base antara base dan branch
        let mb = match run_git_cmd(repo_root, &["merge-base", base_branch, ticket_branch]) {
            Ok(out) => out.trim().to_string(),
            Err(_) => return false,
        };
        
        // Ambil daftar file yang diubah pada branch
        let changed_files = match run_git_cmd(repo_root, &["diff", "--name-only", &mb, ticket_branch]) {
            Ok(out) => out.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>(),
            Err(_) => return false,
        };
        
        if changed_files.is_empty() {
            return true;
        }
        
        // Verifikasi apakah seluruh file tersebut sudah identik di base branch
        let mut diff_args = vec!["diff", "--quiet", ticket_branch, base_branch, "--"];
        for f in &changed_files {
            diff_args.push(f.as_str());
        }
        run_git_cmd_status(repo_root, &diff_args)
    }
    
    pub fn clean_worktree(repo_root: &Path, ticket: &Ticket, force: bool) -> Result<(), RigError> {
        if !force && !Self::has_landed(repo_root, &ticket.base, &ticket.branch) {
            return Err(RigError::UnmergedWorktree(ticket.branch.clone()));
        }
        
        // Hapus worktree dari git
        let _ = run_git_cmd(repo_root, &["worktree", "remove", "--force", &ticket.worktree]);
        
        // Windows locking fallback: retry penghapusan folder sisa hingga 5 detik
        let wt_dir = Path::new(&ticket.worktree);
        if wt_dir.exists() {
            for _ in 0..10 {
                if std::fs::remove_dir_all(wt_dir).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
        
        // Hapus branch lokal
        let _ = run_git_cmd(repo_root, &["branch", "-D", &ticket.branch]);
        Ok(())
    }
}
```

### 4.4 Engine Pengawasan Agen & Detektor Turn (`watcher.rs`)
Menghubungkan `temp-terminal-ui` dengan pengawasan otomatis tanpa beban polling.
`temp-terminal` memiliki akses langsung ke buffer ConPTY. Saat agen menyelesaikan langkah (*settled*):
- Jika agen mengirim pesan via `rig say`: Watcher mencatat laporan berhasil.
- Jika agen berhenti (`idle`, `blocked`, `exited`) tanpa mengirim pesan: Watcher mengekstrak 12 baris terakhir dari buffer virtual screen dan langsung meneruskannya ke Manager:
  `"[watch] worker run #1 is idle without reporting. Screen:\n<tail_content>"`.

### 4.5 Alokasi Layout Terminal Otomatis (`layout.rs`)
Mengadaptasi algoritma `slot()` dari herdr-rig asli untuk layout docking egui:
1. Sesi **Manager** selalu menempati sisi kiri layar penuh (lebar 50%).
2. **Board** membuka di sisi kanan atas (lebar 50%, tinggi 50%).
3. **Worker runs** bertumpuk di bawah Board hingga batas `pane.max` (default 2).
4. Sesi berlebih dialokasikan ke tab overflow (`over-1`, `over-2`) dengan layout 2 kolom (maksimal `tab.max` = 8 pane) yang membagi ruang secara simetris.

---

## 5. Rancangan Komponen UI: Panel Board Native (`temp-terminal-ui`)

Di dalam `temp-terminal-ui`, kita membuat widget panel `RigBoardPanel`:
- **Header**: Logo `▲ rig`, nama repositori, waktu pembaruan terakhir, chip ringkasan status, dan tombol aksi cepat (`+ New Ticket`, `Settings`).
- **Group Accordion**:
  1. `Needs you` (Warna aksen merah/peringatan `theme::STATUS_ERROR`, dot `●`, expanded default).
  2. `Working` (Warna aksen kuning/amber `theme::STATUS_WARNING`, animasi spinner `⠋..⠏`, expanded default).
  3. `Ready` (Warna aksen hijau `theme::STATUS_OK`, dot `◆`).
  4. `Queued` (Warna netral faint `theme::TEXT_FAINT`, dot `◷`).
  5. `To do` (Warna netral muted `theme::TEXT_MUTED`, dot `○`).
  6. `Done` (Warna redup, icon `✓`, collapsed default).
- **Ticket Card Item**:
  - Border kartu 6px radius (`theme::SURFACE_CARD`, border `theme::BORDER_SUBTLE`).
  - ID Tiket (bold), Judul, Badge branch (`⎇ rig/T-n`), Badge Harness (Claude, Codex, dll. dengan warna identitas masing-masing).
  - Indikator gauge effort (4 batang vertikal tipis).
  - Catatan progres terkini (*activity note* atau status pengerjaan).
  - Tombol aksi kontekstual:
    - Jika `Needs you`: Tombol "Answer" (langsung fokus ke pane manager) dan tombol "Open Pane".
    - Jika `Ready`: Tombol "Review".
    - Jika `Working`: Tombol "Steer" dan tombol "Open Pane".
- **Activity Stream Drawer**:
  - Menampilkan 50 pesan terakhir antar-agen dengan format: `HH:MM · [T-n] From -> To: Message`.

---

## 6. Protokol Keterampilan Agen (Skills Ecosystem)

Empat keterampilan utama akan dipasang ke dalam sistem agen `temp-terminal`:

### 6.1 `rig-manager`
- Berperan sebagai koordinator tunggal dalam satu repositori.
- Mengubah instruksi tingkat tinggi pengguna menjadi tiket terstruktur (*Goal, Acceptance, Verify, Scope, Context*).
- Menjalankan setup default satu kali (`worker`, `reviewer`, `mode`, `worktree`, `autonomy`, `merge`).
- Mendengarkan pesan via `rig inbox --wait` tanpa menginterupsi terminal pengguna saat pengguna mengetik.
- Menerapkan **Triage Ringkas**: hanya membaca `git diff --stat` dan menugaskan Reviewer independen untuk perubahan kode besar.

### 6.2 `rig-worker`
- Berperan sebagai pelaksana satu tiket.
- Bekerja secara eksklusif di dalam direktori worktree terisolasi (`.rig/wt/T-n`).
- Melakukan commit berkala, menjalankan seluruh verify command, dan melaporkan status via `rig say manager`.
- Menandai tiket `status=review` saat selesai atau `status=blocked` jika terhambat.

### 6.3 `rig-reviewer`
- Berperan sebagai verifikator kualitas independen (menggunakan keluarga model berbeda dari Worker).
- Bekerja pada worktree yang sama dalam mode read-only.
- Menguji seluruh kriteria penerimaan dan verifikasi command.
- Mengirimkan keputusan tegas: `PASS` atau `FIX: 1) ... 2) ...`.

### 6.4 `rig-orchestrator`
- Berperan mengelola perubahan yang melibatkan banyak repositori sekaligus.
- Mendaftarkan repositori ke hub `~/.rig/hub.db`.
- Membagi brief per repositori dengan kontrak antarmuka yang identik dan ID Story (misal: `S-12`).
- Menyalakan Manager di setiap repositori via `rig launch` dan memantau status agregat di papan orkestrator.

---

## 7. Peta Jalan Implementasi Bertahap (*Phased Roadmap*)

Untuk menambahkan fitur-fitur ini ke dalam `temp-terminal`, proses implementasi dibagi ke dalam 5 fase bertahap:

```text
Fase 1: State & Database Engine (crates/temp-terminal-rig)
        ├── SQLite engine (rig.db & hub.db, WAL, schema)
        ├── Ticket CRUD & dependency queueing (--after)
        └── Unit tests model data dan query

Fase 2: Git Worktree & Landing Manager
        ├── Worktree add/remove automation
        ├── Merge-base diff verification (landed() logic)
        ├── Squash merge runner & cleanup command
        └── Integration tests git worktree

Fase 3: Process Launcher, Inter-Agent Messaging & Watcher
        ├── Harness argv builder (Claude, Codex, OpenCode, Pi, dll.)
        ├── Message bus, say, inbox --wait, quiet delivery
        ├── Background turn watcher & screen tail extractor
        └── Headless & pane supervision

Fase 4: Antarmuka Board (TUI + GUI Widget)
        ├── TUI streaming board (ANSI, spinner, groups)
        ├── Egui native RigBoardPanel di temp-terminal-ui
        ├── Toast alerts pada status needs-you/blocked
        └── Integrasi auto-layout split pane di Workspace

Fase 5: Skills Deployment & Cross-Repo Orchestration
        ├── Pemasangan 4 skill (manager, orchestrator, worker, reviewer)
        ├── Hub database & story aggregator
        ├── End-to-end multi-agent workflow verification
        └── Dokumentasi panduan pengguna
```

---

## 8. Panduan Kepatuhan Anti-Slop & Standar Mutu

1. **Nol Karakter Em Dash**: Dilarang keras menggunakan karakter em dash (\u2014) di seluruh kode Rust, teks UI, komentar, maupun dokumentasi pendukung. Gunakan standar tanda pisah `-`, titik dua `:`, atau titik tengah `·`.
2. **Kepatuhan Token Desain Terpusat**: Seluruh pewarnaan pada panel GUI dan TUI wajib menggunakan token terpusat (`crate::theme::*`), tanpa hardcoded raw RGB (`Color32::from_rgb`).
3. **Nol Emoji Dekoratif**: Icon pencarian, status, dan navigasi menggunakan vector painter native (`Icons::draw_*`) atau simbol tipografi terukur.
4. **Keamanan Eksekusi Windows**: Penanganan locking folder kerja pada Windows wajib dilengkapi mekanisme toleransi retry asinkron dan sweep pembersihan sisa.

---

## 9. Kesimpulan & Langkah Eksekusi

Dokumen cetak biru ini, bersama dengan [`replica/recon.md`](file:///D:/project/myproject/rewrite-terminal/temp-terminal/replica/recon.md) dan [`replica/features.csv`](file:///D:/project/myproject/rewrite-terminal/temp-terminal/replica/features.csv), telah memetakan seluruh kapabilitas `herdr-rig` secara presisi dan mendalam.

Dengan arsitektur ini, `temp-terminal` dapat berevolusi dari sekadar terminal emulator multi-pane biasa menjadi **Integrated Agentic Terminal Environment**: sebuah workspace terpadu di mana pengguna dapat mengorkestrasikan tim agen AI pembuat kode dengan visibilitas penuh, pengawasan hening, dan kontrol kualitas independen.
