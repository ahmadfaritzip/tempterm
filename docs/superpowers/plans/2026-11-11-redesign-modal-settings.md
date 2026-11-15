# Implementation Plan: Redesain Modal Settings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merancang ulang modal Settings (`Ctrl+,`) di `temp-terminal-ui` dengan standar Anti-Slop tinggi, penghapusan total emoji, pengelompokan semantik 11 kategori, grouped setting cards, sinkronisasi cheatsheet shortcut ber-keycaps individual, dan navigasi terpadu.

**Architecture:** Memperkuat `SettingsCategory` dengan semantic grouping (`SettingsCategoryGroup`) dan metadata deskriptif, melengkapi `SettingsModalState` dengan method navigasi murni, membagi tata letak modal menjadi dua kolom (Sidebar terstruktur dan Content Pane ber-grouped cards), mengintegrasikan vector search icon dan keycaps pada tab Keyboard, menyatukan header dan footer terpadu, serta memastikan kepatuhan penuh terhadap token `crate::theme::*`.

**Tech Stack:** Rust, eframe/egui 0.28, tokio.

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh diubah.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-ui/`.
- DILARANG menggunakan karakter em dash di seluruh antarmuka, komentar kode, dan dokumentasi (Anti-Slop R-02).
- DILARANG menggunakan literal emoji (seperti ⚙, 🎨, 🔍, dll.) sebagai dekorasi UI (Anti-Slop R-04).
- Palet aktif dibatasi maksimal 2-3 warna inti netral + 1 warna aksen (Anti-Slop R-29).
- Tidak menggunakan tombol berbentuk pil ekstrem (radius 12+ px), gunakan radius terukur 6.0 px untuk kartu/input dan 4.0 px untuk item tombol/badge (Anti-Slop R-11).
- Semua warna harus menggunakan token tema terpusat di `theme` agar konsisten lintas tema (Anti-Slop R-34).

---

### Task 1: Model Kategori Semantik & State Helper Methods pada SettingsModalState

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`
- Create: `crates/temp-terminal-ui/tests/settings_modal_tests.rs`

**Interfaces:**
- Consumes: `SettingsCategory`, `SettingsModalState`.
- Produces:
  ```rust
  #[derive(Copy, Clone, Debug, PartialEq, Eq)]
  pub enum SettingsCategoryGroup {
      Application,
      Workspace,
      Integrations,
      System,
  }

  impl SettingsCategory {
      pub fn group(&self) -> SettingsCategoryGroup;
      pub fn title(&self) -> &'static str;
      pub fn description(&self) -> &'static str;
      pub fn all() -> &'static [SettingsCategory];
  }

  impl SettingsModalState {
      pub fn open(&mut self);
      pub fn close(&mut self);
      pub fn toggle(&mut self);
      pub fn set_category(&mut self, cat: SettingsCategory);
      pub fn next_category(&mut self);
      pub fn prev_category(&mut self);
  }
  ```

- [ ] **Step 1: Tulis tes unit yang gagal untuk model kategori dan state navigasi**

Buat berkas `crates/temp-terminal-ui/tests/settings_modal_tests.rs`:
```rust
use temp_terminal_ui::workspace::{SettingsCategory, SettingsCategoryGroup, SettingsModalState};

#[test]
fn test_settings_category_groups_and_metadata() {
    let all = SettingsCategory::all();
    assert_eq!(all.len(), 11);

    assert_eq!(SettingsCategory::General.group(), SettingsCategoryGroup::Application);
    assert_eq!(SettingsCategory::Appearance.group(), SettingsCategoryGroup::Application);
    assert_eq!(SettingsCategory::Terminal.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Editor.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Browser.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Keyboard.group(), SettingsCategoryGroup::Workspace);
    assert_eq!(SettingsCategory::Notifications.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::Agents.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::WebAccess.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::DevUrls.group(), SettingsCategoryGroup::Integrations);
    assert_eq!(SettingsCategory::About.group(), SettingsCategoryGroup::System);

    for cat in all {
        assert!(!cat.title().is_empty());
        assert!(!cat.description().is_empty());
    }
}

#[test]
fn test_settings_modal_state_navigation() {
    let mut state = SettingsModalState::default();
    assert!(!state.is_open);

    state.open();
    assert!(state.is_open);

    state.set_category(SettingsCategory::Terminal);
    assert_eq!(state.active_category, SettingsCategory::Terminal);

    state.next_category();
    assert_eq!(state.active_category, SettingsCategory::Editor);

    state.prev_category();
    assert_eq!(state.active_category, SettingsCategory::Terminal);

    state.close();
    assert!(!state.is_open);

    state.toggle();
    assert!(state.is_open);
    state.toggle();
    assert!(!state.is_open);
}
```

- [ ] **Step 2: Jalankan tes unit untuk memastikan kegagalan (Red)**

Jalankan:
```powershell
cargo test --test settings_modal_tests
```
Verifikasi kegagalan kompilasi karena method `all()`, `group()`, `title()`, `description()`, `open()`, `close()`, `toggle()`, `next_category()`, dan `prev_category()` belum ada.

- [ ] **Step 3: Implementasikan pengelompokan kategori dan method state di workspace.rs (Green)**

Edit `crates/temp-terminal-ui/src/workspace.rs`:
Tambahkan `SettingsCategoryGroup`, implementasikan method pada `SettingsCategory` dan `SettingsModalState`.

- [ ] **Step 4: Jalankan tes unit untuk memastikan sukses (Pass)**

Jalankan:
```powershell
cargo test --test settings_modal_tests
```
Pastikan seluruh tes lolos.

- [ ] **Step 5: Commit perubahan Task 1**

```powershell
git add crates/temp-terminal-ui/src/workspace.rs crates/temp-terminal-ui/tests/settings_modal_tests.rs
git commit -m "feat(ui): add semantic category grouping and pure state helpers for settings modal"
```

---

### Task 2: Frame Window, Modal Header, Footer Bar, dan Navigasi Sidebar Anti-Slop

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `SettingsCategoryGroup`, `SettingsCategory`, `SettingsModalState`, `theme::*`.
- Produces: Rendering visual frame modal settings, header dengan keycaps `Ctrl` `+` `,`, sidebar bebas emoji berindikator aksen 2px, dan footer bar dengan `Esc Close` hint serta aksi Save/Close.

- [ ] **Step 1: Implementasikan Header Bar dan Footer Bar pada Settings Window**

Gantikan blok rendering header dan footer lama di `workspace.rs`:
1. Header Bar:
   - Judul: "Settings" (14pt, tebal, `theme::TEXT_STRONG`).
   - Keycap individual: `Ctrl` `+` `,` menggunakan frame keycap mini.
   - Tombol tutup '×' dengan pointer cursor dan hover highlight.
   - Pemisah horizontal `theme::BORDER_SUBTLE`.
2. Footer Bar:
   - Container bar (tinggi 44px, latar `theme::CARD`, border atas `theme::BORDER_MEDIUM`, rounding bawah 8px).
   - Hint sisi kiri: Keycap `Esc` diikuti teks `Close`.
   - Tombol sisi kanan: Tombol sekunder "Close" dan tombol primer "Save Changes" (`theme::ACCENT` highlight).

- [ ] **Step 2: Implementasikan Sidebar Kategori Semantik Bebas Emoji**

Di kolom sidebar kiri (lebar 180px):
1. Iterasi 4 grup semantik: `APPLICATION`, `WORKSPACE`, `INTEGRATIONS`, `SYSTEM`.
2. Tampilkan header grup kecil huruf besar (9.5pt, `theme::TEXT_FAINT`).
3. Tampilkan item kategori murni tipografi (tanpa emoji):
   - Kategori aktif: Indikator vertikal aksen 2px (`theme::ACCENT`) di sisi kiri, latar `theme::SURFACE_ELEVATED`, teks tebal `theme::TEXT_PRIMARY`.
   - Kategori non-aktif: Latar transparan, teks `theme::TEXT_MUTED`, hover latar `theme::HOVER`.
   - Tinggi baris 28px, rounding 4px, pointer cursor.

- [ ] **Step 3: Uji kompilasi dan kelayakan visual**

Jalankan:
```powershell
cargo check --workspace --all-targets
cargo test -p temp-terminal-ui
```

- [ ] **Step 4: Commit perubahan Task 2**

```powershell
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "feat(ui): implement anti-slop header, semantic sidebar, and footer for settings modal"
```

---

### Task 3: Grouped Setting Cards untuk Tab General, Appearance, Terminal, Editor, Notifications

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `SettingsCategory`, `SettingsModalState`, form controls egui, `theme::*`.
- Produces: Tampilan panel konten rapi dengan grouped setting cards (`theme::SURFACE_CARD`, border `theme::BORDER_SUBTLE`, rounding 6px) untuk 5 tab pertama.

- [ ] **Step 1: Buat fungsi helper kartu pengaturan lokal**

Di `workspace.rs`, buat helper untuk merender kartu pengaturan konsisten:
```rust
fn render_setting_card<R>(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R { ... }
```
Menggunakan latar `theme::SURFACE_CARD`, border `theme::BORDER_SUBTLE`, rounding 6px, padding dalam 12px, dan judul kartu 12pt tebal (`theme::TEXT_STRONG`).

- [ ] **Step 2: Refactor konten Tab General dan Appearance**

1. Tab General:
   - Kartu "Session Lifecycle": checkbox daemon, restore sessions, resume agents, confirm close.
   - Kartu "Link Handling": dropdown pilihan browser eksternal sistem vs browser pane internal.
2. Tab Appearance:
   - Kartu "Theme & Layout": dropdown Theme (Dark, Light, Omarchy) dan Density (Comfortable, Compact).
   - Kartu "Display Options": checkbox dim unfocused panes dan show system load.

- [ ] **Step 3: Refactor konten Tab Terminal, Editor, dan Notifications**

1. Tab Terminal:
   - Kartu "Typography": TextEdit font family dan Slider font size (10.0..=24.0 pt).
   - Kartu "Cursor & Interaction": ComboBox cursor shape (Block, Bar, Underline), toggle cursor blink, toggle copy on select.
   - Kartu "Scrollback Buffer": Slider scrollback limit (1,000..=50,000).
2. Tab Editor:
   - Kartu "Formatting & Display": Slider tab size (2..=8), toggle word wrap, toggle line numbers, toggle diff split.
3. Tab Notifications:
   - Kartu "Desktop Alerts": checkbox notify waiting, notify done, notify error, notify sound.

- [ ] **Step 4: Uji kompilasi dan eksekusi tes unit**

Jalankan:
```powershell
cargo check --workspace --all-targets
cargo test -p temp-terminal-ui
```

- [ ] **Step 5: Commit perubahan Task 3**

```powershell
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "feat(ui): apply grouped cards to general, appearance, terminal, editor, and notification settings"
```

---

### Task 4: Grouped Cards & Refactoring untuk Tab Agents, Web Access, Dev URLs, Browser, Keyboard, About

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`
- Modify: `crates/temp-terminal-ui/tests/settings_modal_tests.rs`

**Interfaces:**
- Consumes: `get_all_shortcuts()`, `filter_shortcuts()`, `Icons::draw_search`, `SettingsCategory`.
- Produces: Tampilan panel konten berstandar Anti-Slop untuk 6 tab lanjutan, termasuk tabel agents, status restriksi platform, browser search settings, sinkronisasi cheatsheet keyboard ber-keycaps, dan kartu About.

- [ ] **Step 1: Refactor Tab Agents, Web Access, Dev URLs, dan Browser**

1. Tab Agents:
   - Kartu "Supervised CLI Integrations": tabel status tool (Claude Code, Pi, OpenCode, Codex, Aider) berlatar netral dengan badge status `STATUS_OK`.
   - Kartu "Permissions & Sockets": checkbox control socket dan remote control.
2. Tab Web Access & Dev URLs:
   - Kartu "Platform Availability": Banner restriksi Windows dengan badge status netral "Unavailable on Windows" dan penjelasan daemon (R-26, R-38).
3. Tab Browser:
   - Kartu "Search Engine": ComboBox provider search + text field custom search URL jika opsi Custom dipilih.
   - Kartu "Browsing Data": Deskripsi dan tombol sekunder "Clear Browsing Data".

- [ ] **Step 2: Refactor Tab Keyboard dengan Vector Search dan Individual Keycaps**

1. Gantikan emoji `🔍` dengan `Icons::draw_search(ui, search_rect, theme::TEXT_MUTED)`.
2. Sinkronkan data shortcut dengan `get_all_shortcuts()` dan fungsi filter `filter_shortcuts()`.
3. Tampilkan setiap baris shortcut dengan:
   - Badge kategori netral (`theme::CARD`).
   - Judul shortcut (`theme::TEXT_NORMAL`).
   - Keycap individual untuk setiap kombinasi tombol (misal `Ctrl` `+` `Shift` `+` `D`).

- [ ] **Step 3: Refactor Tab About**

1. Kartu "Build Information": Nama aplikasi, versi `v0.1.0-preview`, catatan non-self-updating.
2. Kartu "Storage Paths": Direktori Settings, State, dan Logs dengan tombol salin path.
3. Kartu "Official Links": Hyperlink eksternal resmi (Website, What's New, Report Issue, Credits).

- [ ] **Step 4: Tambahkan tes integrasi tab Keyboard pada settings_modal_tests.rs**

Tambahkan pengujian bahwa shortcut yang tersedia di tab Keyboard Settings selaras dengan cheatsheet:
```rust
#[test]
fn test_settings_keyboard_tab_uses_canonical_shortcuts() {
    let all = temp_terminal_ui::workspace::get_all_shortcuts();
    assert!(all.len() >= 27);
    let filtered = temp_terminal_ui::workspace::filter_shortcuts(&all, "terminal", None);
    assert!(!filtered.is_empty());
}
```

- [ ] **Step 5: Uji kompilasi dan pastikan seluruh tes lolos**

Jalankan:
```powershell
cargo check --workspace --all-targets
cargo test -p temp-terminal-ui
```

- [ ] **Step 6: Commit perubahan Task 4**

```powershell
git add crates/temp-terminal-ui/src/workspace.rs crates/temp-terminal-ui/tests/settings_modal_tests.rs
git commit -m "feat(ui): modernize agents, platform warnings, browser, keyboard keycaps, and about tab"
```

---

### Task 5: Validasi Menyeluruh, Audit Anti-Slop (Nol Em Dash, Nol Emoji, Nol Raw Colors), dan Kompilasi Binary

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs` (jika ada pembersihan sisa)

**Interfaces:**
- Produces: Binary GUI terverifikasi `temp-terminal-gui.exe`, audit bersih Anti-Slop 100%.

- [ ] **Step 1: Audit Nol Em Dash di seluruh file repositori**

Jalankan:
```powershell
Select-String -Path "crates/temp-terminal-ui/src/workspace.rs" -Pattern "\u2014"
```
Pastikan 0 kecocokan.

- [ ] **Step 2: Audit Nol Emoji Literal pada Modal Settings**

Verifikasi tidak ada emoji literal (`⚙`, `🎨`, `⌨`, `🔍`, dll.) di bagian Settings modal.

- [ ] **Step 3: Audit Nol Raw Color di Bagian Settings Modal**

Pastikan seluruh warna menggunakan `theme::*` dan tidak ada `Color32::from_rgb`.

- [ ] **Step 4: Jalankan seluruh test suite**

Jalankan:
```powershell
cargo check --workspace --all-targets
cargo test -p temp-terminal-ui
```
Pastikan 100% tes lulus.

- [ ] **Step 5: Kompilasi binary debug temp-terminal-gui**

Jalankan:
```powershell
cargo build -p temp-terminal-gui
```
Pastikan file `target/debug/temp-terminal-gui.exe` terkompilasi dengan sukses tanpa error dan tanpa warning.

- [ ] **Step 6: Commit final validasi**

```powershell
git add -u
git commit -m "chore(ui): finalize validation, anti-slop audit, and binary compilation for settings modal"
```
