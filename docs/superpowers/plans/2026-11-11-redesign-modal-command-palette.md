# Implementation Plan: Redesain Modal Command Palette

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merancang ulang modal Command Palette (`Ctrl+Shift+P`) di `temp-terminal-ui` dengan standar Anti-Slop tinggi, kepatuhan token tema terpusat, rendering keycaps individual, auto-scroll dinamis, dan navigasi keyboard yang mulus.

**Architecture:** Memperkuat `CommandPaletteState` dengan method navigasi dan boundary safety murni, memisahkan logika penyaringan `filter_palette_commands`, menggantikan emoji pencarian dengan `Icons::draw_search`, menerapkan highlight seleksi baris `theme::SELECTED`, badge kategori netral, keycaps individual, footer bar, dan auto-scroll via `ui.scroll_to_rect`.

**Tech Stack:** Rust, eframe/egui 0.28, tokio.

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh diubah.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-ui/`.
- DILARANG menggunakan karakter em dash di seluruh antarmuka dan teks bantuan (Anti-Slop R-02).
- DILARANG menggunakan literal emoji (seperti 🔍) sebagai dekorasi UI (Anti-Slop R-04).
- Palet aktif dibatasi maksimal 2-3 warna inti netral + 1 warna aksen (Anti-Slop R-29).
- Tidak menggunakan tombol berbentuk pil ekstrem (radius 12+ px), gunakan radius terukur 6.0 px (Anti-Slop R-11).
- Semua warna harus menggunakan token tema terpusat di `theme` agar konsisten lintas tema (Anti-Slop R-34).

---

### Task 1: Data Model & Logika Filter Murni Command Palette dengan Unit Tests

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`
- Create: `crates/temp-terminal-ui/tests/command_palette_tests.rs`

**Interfaces:**
- Consumes: `CommandPaletteState`, `PaletteCommand`.
- Produces:
  ```rust
  impl CommandPaletteState {
      pub fn open(&mut self);
      pub fn close(&mut self);
      pub fn toggle(&mut self);
      pub fn move_up(&mut self, total_items: usize);
      pub fn move_down(&mut self, total_items: usize);
      pub fn clamp_selection(&mut self, total_items: usize);
  }
  pub fn filter_palette_commands<'a>(commands: &'a [PaletteCommand], query: &str) -> Vec<&'a PaletteCommand>;
  ```

- [ ] **Step 1: Tulis tes unit yang gagal untuk CommandPaletteState dan filter**

Buat berkas `crates/temp-terminal-ui/tests/command_palette_tests.rs`:
```rust
use temp_terminal_ui::workspace::{filter_palette_commands, CommandPaletteState, PaletteCommand};

#[test]
fn test_command_palette_state_navigation() {
    let mut state = CommandPaletteState::default();
    assert!(!state.is_open);

    state.open();
    assert!(state.is_open);
    assert_eq!(state.selected_index, 0);
    assert!(state.query.is_empty());

    // Navigation down with wrap around
    state.move_down(3);
    assert_eq!(state.selected_index, 1);
    state.move_down(3);
    assert_eq!(state.selected_index, 2);
    state.move_down(3);
    assert_eq!(state.selected_index, 0); // wrapped around

    // Navigation up with wrap around
    state.move_up(3);
    assert_eq!(state.selected_index, 2); // wrapped to bottom
    state.move_up(3);
    assert_eq!(state.selected_index, 1);
    state.move_up(3);
    assert_eq!(state.selected_index, 0);

    // Clamping boundaries
    state.selected_index = 10;
    state.clamp_selection(5);
    assert_eq!(state.selected_index, 0);

    state.close();
    assert!(!state.is_open);
}

#[test]
fn test_filter_palette_commands() {
    let cmds = [
        PaletteCommand { id: "session.new", title: "New Terminal Session", category: "Session", shortcut: Some("Ctrl+T") },
        PaletteCommand { id: "file.save", title: "Save File", category: "File", shortcut: Some("Ctrl+S") },
        PaletteCommand { id: "view.toggleSidebar", title: "Toggle Left Sidebar", category: "View", shortcut: Some("Ctrl+B") },
    ];

    // Match title
    let res = filter_palette_commands(&cmds, "terminal");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "session.new");

    // Match category
    let res = filter_palette_commands(&cmds, "File");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "file.save");

    // Match shortcut
    let res = filter_palette_commands(&cmds, "Ctrl+B");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "view.toggleSidebar");

    // No match
    let res = filter_palette_commands(&cmds, "UnknownAction");
    assert!(res.is_empty());
}
```

- [ ] **Step 2: Jalankan tes untuk memverifikasi gagal**

Jalankan: `cargo test -p temp-terminal-ui --test command_palette_tests`
Ekspektasi: GAGAL karena method dan fungsi filter belum diimplementasikan.

- [ ] **Step 3: Implementasikan method pada CommandPaletteState dan fungsi filter di workspace.rs**

Di `crates/temp-terminal-ui/src/workspace.rs`, lengkapi `CommandPaletteState` dan fungsi `filter_palette_commands`:
```rust
impl CommandPaletteState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
        self.selected_index = 0;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn move_up(&mut self, total_items: usize) {
        if total_items > 0 {
            if self.selected_index == 0 {
                self.selected_index = total_items - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn move_down(&mut self, total_items: usize) {
        if total_items > 0 {
            self.selected_index = (self.selected_index + 1) % total_items;
        }
    }

    pub fn clamp_selection(&mut self, total_items: usize) {
        if total_items == 0 || self.selected_index >= total_items {
            self.selected_index = 0;
        }
    }
}

pub fn filter_palette_commands<'a>(
    commands: &'a [PaletteCommand],
    query: &str,
) -> Vec<&'a PaletteCommand> {
    let q = query.trim().to_lowercase();
    commands
        .iter()
        .filter(|c| {
            if q.is_empty() {
                true
            } else {
                c.title.to_lowercase().contains(&q)
                    || c.category.to_lowercase().contains(&q)
                    || c.shortcut.map_or(false, |sc| sc.to_lowercase().contains(&q))
            }
        })
        .collect()
}
```

- [ ] **Step 4: Jalankan tes unit untuk memastikan lulus**

Jalankan: `cargo test -p temp-terminal-ui --test command_palette_tests`
Ekspektasi: LULUS (semua tes hijau).

- [ ] **Step 5: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs crates/temp-terminal-ui/tests/command_palette_tests.rs
git commit -m "feat(ui): add data model and pure filtering logic for command palette"
```

---

### Task 2: Redesain Framing Modal & Bilah Pencarian Terintegrasi

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `theme::POPOVER`, `theme::BORDER_MEDIUM`, `theme::SURFACE_INPUT`, `theme::TEXT_MUTED`, `Icons::draw_search`.
- Produces: Window mengambang di `CENTER_TOP`, eliminasi emoji 🔍 dengan `Icons::draw_search`, live counter.

- [ ] **Step 1: Perbarui framing Window Command Palette**

Ubah konfigurasi window `egui::Window::new("Command Palette")`:
- Posisi: `anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 70.0))`
- Ukuran: `fixed_size(Vec2::new(640.0, 420.0))`
- Frame:
  - Fill: `theme::SURFACE_OVERLAY` (`theme::POPOVER`)
  - Stroke: `Stroke::new(1.0_f32, theme::BORDER_MEDIUM)`
  - Rounding: `Rounding::same(8.0_f32)`
  - Shadow: satu lapis halus yang konsisten

- [ ] **Step 2: Redesain Search Input Bar**

Ganti blok input pencarian lama:
- Hilangkan emoji `"🔍"` (Anti-Slop R-04).
- Gunakan kontainer input:
  - Fill: `theme::SURFACE_INPUT` (`theme::CARD`)
  - Stroke: `Stroke::new(1.0_f32, theme::BORDER_MEDIUM)`
  - Rounding: `Rounding::same(6.0_f32)`
- Gambar ikon: `Icons::draw_search(ui.painter(), icon_r, theme::TEXT_MUTED)`
- TextEdit:
  - Hint text: `"Type a command or action name…"`
  - Auto-request focus
  - Deteksi perubahan: jika `resp.changed()`, panggil `self.command_palette.selected_index = 0`
- Counter label di kanan: `format!("{} commands", filtered.len())` warna `theme::TEXT_MUTED`

- [ ] **Step 3: Sinkronkan handler pembukaan dan penutupan**

Periksa `workspace.rs`: ganti mutasi manual `self.command_palette.is_open` dengan `open()`, `close()`, atau `toggle()` pada event `Ctrl+Shift+P` dan tombol menu.

- [ ] **Step 4: Verifikasi kompilasi dan tes**

Jalankan:
- `cargo check -p temp-terminal-ui`
- `cargo test -p temp-terminal-ui --test command_palette_tests`

- [ ] **Step 5: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "refactor(ui): update command palette framing and search bar to theme tokens"
```

---

### Task 3: Redesain Baris Perintah, Badge Kategori Netral, Keycaps Individual, & Footer Bar

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `filter_palette_commands`, `theme` tokens.
- Produces: Baris perintah dengan highlight aktif/hover, badge kategori netral (Anti-Slop R-29), keycaps individual, empty state informatif, dan footer bar.

- [ ] **Step 1: Redesain Baris Perintah dan Status Seleksi**

Di dalam `ScrollArea::vertical()`:
- Panggil `filter_palette_commands(&all_cmds, &self.command_palette.query)`
- Setiap baris dialokasikan dengan tinggi 34.0 px:
  - Baris terpilih (`is_sel`): Fill `theme::SELECTED`, stroke `Stroke::new(1.0_f32, theme::ACCENT)`, teks judul `Color32::WHITE`.
  - Baris hover: Fill `theme::HOVER`, teks judul `theme::TEXT_STRONG`.
  - Baris normal: Teks judul `theme::TEXT_NORMAL`.
  - Kursor `PointingHand` pada interaksi hover.

- [ ] **Step 2: Badge Kategori Netral & Keycaps Individual**

- Badge Kategori:
  - Fill: `theme::CARD` (atau `theme::SURFACE_OVERLAY` saat terpilih)
  - Stroke: `theme::BORDER_SUBTLE` (atau `theme::ACCENT` saat terpilih)
  - Text: `theme::TEXT_MUTED` (atau `Color32::WHITE` saat terpilih)
  - Rounding: `Rounding::same(4.0_f32)`
- Keycaps Pintasan:
  - Uraikan `cmd.shortcut` menjadi tombol individual (`Ctrl`, `Shift`, `P`).
  - Keycap normal: Latar `theme::CARD`, stroke `theme::BORDER_STRONG`, teks monospace `theme::TEXT`.
  - Keycap terpilih: Latar `theme::ACCENT`, stroke `theme::ACCENT_HOVER`, teks putih `Color32::WHITE`.
  - Rounding: `Rounding::same(4.0_f32)`.

- [ ] **Step 3: Redesain Empty State & Footer Bar**

- Empty State jika `filtered.is_empty()`:
  - Judul: `"No matching commands"` (`theme::TEXT_FAINT`, font `13.0`)
  - Petunjuk: `"Try typing a different command name or keyword"` (`theme::TEXT_MUTED`, font `11.5`)
- Footer Bar:
  - Background: `theme::CARD`
  - Border atas: `Stroke::new(1.0_f32, theme::BORDER_SUBTLE)`
  - Rounding bawah: `Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 8.0 }`
  - Panduan: `"↑↓ Navigate"`, `"·"`, `"↵ Run Command"`, `"·"`, `"Esc Close"` dengan warna `theme::TEXT_MUTED`.

- [ ] **Step 4: Verifikasi kompilasi dan tes**

Jalankan:
- `cargo check -p temp-terminal-ui`
- `cargo test -p temp-terminal-ui`

- [ ] **Step 5: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "feat(ui): style command palette rows, neutral badges, keycaps, and footer"
```

---

### Task 4: Implementasi Auto-Scroll & Navigasi Keyboard Penuh

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`
- Modify: `crates/temp-terminal-ui/tests/command_palette_tests.rs`

**Interfaces:**
- Consumes: Input event `ArrowUp`, `ArrowDown`, `Enter`, `Escape`.
- Produces: Auto-scroll tracking ke baris aktif, penanganan panah wrap-around, eksekusi perintah aman, dan unit test alur interaksi.

- [ ] **Step 1: Terapkan auto-scroll pada baris terpilih**

Di dalam loop perenderan baris:
```rust
if is_sel {
    ui.scroll_to_rect(padded_rect, Some(egui::Align::Center));
}
```

- [ ] **Step 2: Penanganan tombol panah, Enter, dan Escape**

- `ArrowDown`: `self.command_palette.move_down(filtered.len());`
- `ArrowUp`: `self.command_palette.move_up(filtered.len());`
- `clamp_selection`: panggil `self.command_palette.clamp_selection(filtered.len());`
- `Enter`: ambil `execute_cmd_id = Some(cmd.id)`, panggil `self.command_palette.close()`, dan panggil `self.execute_command(id)` di luar blok window.
- `Escape`: panggil `self.command_palette.close()`.

- [ ] **Step 3: Tambahkan pengujian unit alur interaksi dan wrap-around**

Di `crates/temp-terminal-ui/tests/command_palette_tests.rs`, tambahkan tes untuk wrap-around navigasi, clamping dengan query berubah, dan format parsing keycaps pada `get_palette_commands`.

- [ ] **Step 4: Verifikasi kompilasi dan tes**

Jalankan:
- `cargo check -p temp-terminal-ui`
- `cargo test -p temp-terminal-ui`

- [ ] **Step 5: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs crates/temp-terminal-ui/tests/command_palette_tests.rs
git commit -m "feat(ui): implement auto-scroll and seamless navigation for command palette"
```

---

### Task 5: Validasi Menyeluruh & Audit Anti-Slop Akhir

**Files:**
- Test: Seluruh suite tes di workspace (`cargo check --workspace`, `cargo test -p temp-terminal-ui`).
- Build: `cargo build -p temp-terminal-ui --bin temp-terminal-gui`.

- [ ] **Step 1: Jalankan kompilasi penuh workspace**

Jalankan: `cargo check --workspace --all-targets`
Ekspektasi: 0 error, 0 warning.

- [ ] **Step 2: Jalankan seluruh suite tes unit dan integrasi**

Jalankan: `cargo test -p temp-terminal-ui`
Ekspektasi: 100% lulus.

- [ ] **Step 3: Audit checklist Anti-Slop (Delivery Gate)**

Verifikasi:
- R-01 & R-29: 0 warna pelangi acak.
- R-02: 0 karakter em dash (`\u2014`).
- R-04: 0 emoji literal (tidak ada lagi `"🔍"`).
- R-11: Radius teratur (8px, 6px, 4px), tidak ada pill ekstrem.
- R-25: Kontras teks memenuhi WCAG AA.
- R-27: Empty state informatif jika hasil pencarian kosong.
- R-32 & R-39: Aksesibilitas keyboard dan kursor interaktif.
- R-34: Sepenuhnya dinamis mengacu ke token tema.

- [ ] **Step 4: Kompilasi biner aplikasi GUI**

Jalankan: `cargo build -p temp-terminal-ui --bin temp-terminal-gui`
Ekspektasi: Selesai dengan sukses (exit code 0).

- [ ] **Step 5: Commit dokumentasi final**

```bash
git add docs/superpowers/plans/2026-11-11-redesign-modal-command-palette.md
git commit -m "docs: finalize implementation plan for command palette modal redesign"
```
