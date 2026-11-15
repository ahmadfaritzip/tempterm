# Spesifikasi Desain: Redesain Modal Settings

- **Tanggal**: 2026-11-11
- **Status**: Disetujui
- **Komponen Target**: `crates/temp-terminal-ui` (`workspace.rs`, `SettingsCategory`, `SettingsModalState`)

---

## 1. Latar Belakang & Tujuan
Modal Settings (`Ctrl+,`) saat ini memiliki beberapa masalah visual, hierarki, dan ergonomi:
1. Navigasi sidebar menggunakan emoji literal (`"⚙ General"`, `"🎨 Appearance"`, `"⌨ Terminal"`, `"📝 Editor"`, `"🔔 Notifications"`, `"🤖 Agents"`, `"📱 Web Access"`, `"🌐 Dev URLs"`, `"🌍 Browser"`, `"🔤 Keyboard"`, `"ℹ About"`) yang melanggar aturan Anti-Slop R-04 (Emoji as Decoration).
2. Bilah pencarian shortcut pada tab Keyboard menggunakan emoji literal `"🔍"`, juga melanggar R-04.
3. Struktur sidebar kategori berupa daftar tombol datar tanpa pengelompokan hierarki semantik, tanpa indikator visual yang jelas untuk kategori aktif.
4. Panel konten sebelah kanan menampilkan kontrol form (checkbox, dropdown, textfield, slider) secara bertumpuk tanpa kartu pengelompokan (*grouped setting cards*), sehingga form terasa tercerai-berai.
5. Tab Keyboard menduplikasi daftar shortcut secara statis (18 item) yang tidak sinkron dengan cheatsheet shortcut utama (`get_all_shortcuts()`, 27 item), serta belum menggunakan keycaps individual.
6. Footer bar hanya memiliki dua tombol teks polos ("Close" dan "Save") tanpa pemisahan peran tombol primer/sekunder dan tanpa panduan shortcut keyboard (`Esc`).

Tujuan perancangan ulang ini adalah menghadirkan modal Settings yang berstandar tinggi (*craftsmanship*), mematuhi sistem token terpusat (`crate::theme::*`), bebas emoji, memiliki pengelompokan semantik, kartu form terstruktur, keycaps individual pada tab Keyboard, dan navigasi terpadu.

---

## 2. Arsitektur & Struktur State

### 2.1 Pengelompokan Kategori Semantik (`SettingsCategory`)
Sebanyak 11 kategori bawaan dari Go `tempterm/settings_pages.go` dikelompokkan ke dalam 4 grup semantik:
- **Application**: General, Appearance
- **Workspace**: Terminal, Editor, Browser, Keyboard
- **Integrations**: Notifications, Agents, Web Access, Dev URLs
- **System**: About

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsCategoryGroup {
    Application,
    Workspace,
    Integrations,
    System,
}

impl SettingsCategoryGroup {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Application => "APPLICATION",
            Self::Workspace => "WORKSPACE",
            Self::Integrations => "INTEGRATIONS",
            Self::System => "SYSTEM",
        }
    }
}

impl SettingsCategory {
    pub fn group(&self) -> SettingsCategoryGroup {
        match self {
            Self::General | Self::Appearance => SettingsCategoryGroup::Application,
            Self::Terminal | Self::Editor | Self::Browser | Self::Keyboard => {
                SettingsCategoryGroup::Workspace
            }
            Self::Notifications | Self::Agents | Self::WebAccess | Self::DevUrls => {
                SettingsCategoryGroup::Integrations
            }
            Self::About => SettingsCategoryGroup::System,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
            Self::Notifications => "Notifications",
            Self::Agents => "Agents",
            Self::WebAccess => "Web Access",
            Self::DevUrls => "Dev URLs",
            Self::Browser => "Browser",
            Self::Keyboard => "Keyboard",
            Self::About => "About",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::General => "Session daemon, recovery, and link opening behavior",
            Self::Appearance => "Theme, density, and interface visibility settings",
            Self::Terminal => "Typography, cursor behavior, and scrollback limit",
            Self::Editor => "Tab size, word wrapping, and diff view layout",
            Self::Notifications => "Desktop alerts and notification sounds",
            Self::Agents => "Agent supervision, socket controls, and tool integration",
            Self::WebAccess => "Remote browser access and pairing status",
            Self::DevUrls => "Local domain proxy routing for processes",
            Self::Browser => "Default search provider and browser cache",
            Self::Keyboard => "Cheatsheet and keyboard shortcut configuration",
            Self::About => "Build version, application paths, and project links",
        }
    }
}
```

### 2.2 Model State (`SettingsModalState`)
Model state di `crates/temp-terminal-ui/src/workspace.rs` diperluas dengan method pembantu murni:
```rust
impl SettingsModalState {
    pub fn open(&mut self) {
        self.is_open = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn set_category(&mut self, cat: SettingsCategory) {
        self.active_category = cat;
    }

    pub fn next_category(&mut self) {
        // Navigasi siklik ke kategori berikutnya
    }

    pub fn prev_category(&mut self) {
        // Navigasi siklik ke kategori sebelumnya
    }
}
```

---

## 3. Spesifikasi UI & Visual (Anti-Slop Compliance)

### 3.1 Window Frame
- **Dimensi**: 800 × 540 px (resizable, batas minimum 680 × 460 px), diposisikan di tengah layar (`CENTER_CENTER`).
- **Latar & Bayangan**: `theme::SURFACE_OVERLAY`, border `theme::BORDER_MEDIUM`, rounding 8px.
- **Bayangan**: Bayangan tunggal ber-blur 24px, offset (0, 8px), alpha 160 (R-12: satu bayangan fungsional, tidak ada glow berlebihan R-13).

### 3.2 Header Bar Modal
- **Judul**: "Settings" (`theme::TEXT_STRONG`, 14pt, tebal).
- **Shortcut Keycap**: Badge keycap individual `Ctrl` `+` `,` di sisi judul.
- **Tombol Tutup ('×')**: Latar transparan, efek hover halus, pointer cursor.
- **Pemisah**: Garis horizontal halus `theme::BORDER_SUBTLE`.

### 3.3 Sidebar Kategori (Left Column, lebar 180px)
- **Eliminasi Emoji (R-04)**: Seluruh label kategori murni tipografi tanpa emoji.
- **Header Bagian Semantik**: Label kecil huruf besar (`APPLICATION`, `WORKSPACE`, `INTEGRATIONS`, `SYSTEM`) dengan warna `theme::TEXT_FAINT` (ukuran 9.5pt, tracking rapi).
- **Item Kategori**:
  - Tinggi tombol: 30px, rounding 4px.
  - Tab Aktif: Indikator vertikal 2px berwarna `theme::ACCENT` di tepi kiri, latar `theme::SURFACE_ELEVATED`, teks tebal `theme::TEXT_PRIMARY`.
  - Tab Non-Aktif: Latar transparan, teks `theme::TEXT_MUTED`, hover latar `theme::HOVER`.

### 3.4 Panel Konten (Right Column, ScrollArea vertikal)
- **Header Kategori**: Judul kategori (15pt, `theme::TEXT_STRONG`) dan deskripsi (11.5pt, `theme::TEXT_MUTED`).
- **Kartu Pengelompokan (Grouped Setting Cards)**:
  - Latar: `theme::SURFACE_CARD`, border `theme::BORDER_SUBTLE`, rounding 6px, padding dalam 12px.
  - Judul kartu: 12pt, tebal (`theme::TEXT_STRONG`).
  - Item form: Checkbox, ComboBox, TextEdit, dan Slider dengan tipografi jelas dan helper text `theme::TEXT_MUTED`.
- **Tab Khusus**:
  - **Agents**: Tabel daftar CLI tool (Claude Code, Pi, OpenCode, Codex, Aider) dalam kartu rapi dengan badge status netral `STATUS_OK`.
  - **Web Access & Dev URLs**: Kartu status pembatasan platform Windows dengan badge netral "Unavailable on Windows" tanpa kontrol palsu (R-26, R-38).
  - **Keyboard**: Vector search icon `Icons::draw_search` (R-04), live match counter, dan baris shortcut dengan keycaps individual bersumber langsung dari `get_all_shortcuts()`.
  - **About**: Kartu versi build, daftar path file (dengan tombol salin path), dan hyperlink eksternal resmi.

### 3.5 Footer Bar Modal
- **Dimensi**: Tinggi 44px, latar `theme::CARD`, border atas `theme::BORDER_MEDIUM`, rounding bawah 8px.
- **Sisi Kiri**: Panduan navigasi keyboard `Esc Close` (menggunakan keycap mini).
- **Sisi Kanan**:
  - Tombol Sekunder "Close": Teks `theme::TEXT_MUTED`, hover `theme::HOVER`.
  - Tombol Primer "Save Changes": Latar beraksen `theme::ACCENT` atau border aksen dengan teks kontras.

---

## 4. Rencana Pengujian & Validasi
1. **Unit Tests (`crates/temp-terminal-ui/tests/settings_modal_tests.rs`)**:
   - Uji method state `open()`, `close()`, `toggle()`.
   - Uji navigasi kategori `set_category()`, `next_category()`, `prev_category()`.
   - Uji pengelompokan semantik `SettingsCategory::group()`.
   - Uji kelengkapan 11 kategori dan deskripsi.
   - Uji sinkronisasi shortcut pada tab Keyboard dengan `get_all_shortcuts()`.
2. **Kompilasi & Analisis Statis**:
   - `cargo check --workspace --all-targets` tanpa error dan tanpa warning.
   - `cargo test -p temp-terminal-ui` lulus 100%.
3. **Pemeriksaan Anti-Slop Mutlak**:
   - Nol karakter em dash di seluruh file kode, UI copy, dan komentar (Anti-Slop R-02).
   - Nol emoji literal di seluruh kode UI settings.
   - Nol warna `Color32::from_rgb` mentah di file terkait modal.
