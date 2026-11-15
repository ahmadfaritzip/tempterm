# Spesifikasi Desain: Redesain Modal Command Palette

- **Tanggal**: 2026-11-11
- **Status**: Disetujui
- **Komponen Target**: `crates/temp-terminal-ui` (`workspace.rs`, `CommandPaletteState`, `PaletteCommand`)

---

## 1. Latar Belakang & Tujuan
Modal Command Palette (`Ctrl+Shift+P`) saat ini memiliki beberapa masalah desain, konsistensi, dan ergonomi:
1. Menggunakan emoji literal `"🔍"` di bilah pencarian yang melanggar aturan Anti-Slop R-04 (Emoji as Decoration).
2. Variabel latar seleksi `_fill` didefinisikan tetapi tidak pernah diterapkan ke latar baris; hanya ada border outline tipis saat baris terpilih tanpa latar highlight atau hover state yang memadai.
3. Ketiadaan mekanisme *auto-scroll* pada `ScrollArea`, sehingga baris terpilih dapat keluar dari batas pandang saat pengguna menavigasi dengan panah atas/bawah.
4. Format badge kategori hanya teks kurung biasa (`[App]`), dan shortcut di sisi kanan berupa teks polos redup tanpa keycaps rapi.
5. Ketiadaan footer bar sebagai panduan navigasi interaksi keyboard.

Tujuan perancangan ulang ini adalah menyelaraskan Command Palette dengan standar Anti-Slop tinggi yang telah diterapkan pada Keyboard Shortcuts Modal: tata letak mengambang yang bersih, kepatuhan token tema terpusat, keycaps individual, auto-scroll dinamis, dan kecepatan navigasi berbasis keyboard.

---

## 2. Arsitektur & Struktur State

### 2.1 State Model (`CommandPaletteState`)
Model state di `crates/temp-terminal-ui/src/workspace.rs` diperluas dengan method pembantu murni:
```rust
#[derive(Clone, Debug, Default)]
pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
}

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
```

### 2.2 Model Data & Fungsi Filter Murni
```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteCommand {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
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

---

## 3. Desain Komponen UI & Token Tema

### 3.1 Penataan Modal Mengambang
Modal menggunakan `egui::Window` berukuran tetap (640 x 420 px) yang diposisikan di tengah atas layar (`CENTER_TOP, Vec2::new(0.0, 70.0)`):
- **Frame & Permukaan**: Latar `theme::SURFACE_OVERLAY`, border `theme::BORDER_MEDIUM` (1.0 px), sudut pembulatan 8.0 px.
- **Bayangan Elegan**: Bayangan halus satu lapis untuk mengangkat modal di atas terminal dan editor.

### 3.2 Bilah Pencarian Terintegrasi (Anti-Slop R-04)
- Menggantikan emoji `"🔍"` dengan ikon vektor `Icons::draw_search` berwarna `theme::TEXT_MUTED`.
- Kolom input teks berlatar `theme::SURFACE_INPUT` (`theme::CARD`), border `theme::BORDER_MEDIUM`, pembulatan 6.0 px (bukan pill).
- Auto-focus saat modal dibuka (`resp.request_focus()`).
- Indikator jumlah perintah yang cocok di sisi kanan: `format!("{} commands", filtered.len())` font `11.5` warna `theme::TEXT_MUTED`.
- Reset `selected_index = 0` setiap kali teks pencarian berubah (`resp.changed()`).

### 3.3 Daftar Baris Perintah
- **Tinggi & Padding Baris**: Baris perintah berjarak vertikal 2.0 px dengan margin dalam teratur.
- **Highlight Seleksi & Hover**:
  - Baris terpilih (`is_sel`): Fill `theme::SELECTED`, stroke `theme::ACCENT`, teks judul `Color32::WHITE`.
  - Baris hover: Fill `theme::HOVER`, teks judul `theme::TEXT_STRONG`.
  - Baris normal: Teks judul `theme::TEXT_NORMAL` (`theme::TEXT`).
- **Badge Kategori Netral (Anti-Slop R-29)**:
  - Latar `theme::CARD`, stroke `theme::BORDER_SUBTLE`, teks `theme::TEXT_MUTED` (tanpa warna pelangi).
  - Rounding 4.0 px.
- **Keycaps Individual**:
  - Tombol pintasan jika ada diuraikan menjadi keycap individual (misal: `Ctrl`, `Shift`, `P`).
  - Keycap normal: Latar `theme::CARD`, stroke `theme::BORDER_STRONG`, teks monospace `theme::TEXT`.
  - Keycap baris terpilih: Latar `theme::ACCENT`, stroke `theme::ACCENT_HOVER`, teks putih kontras tinggi.
  - Rounding 4.0 px.
- **Auto-Scroll Aktif**: Memanggil `ui.scroll_to_rect(row_rect, Some(egui::Align::Center))` saat baris terpilih dirender.

### 3.4 State Kosong (Empty State - Anti-Slop R-27)
- Jika `filtered.is_empty()`:
  - Judul: `"No matching commands"` (`theme::TEXT_FAINT`, font `13.0`).
  - Petunjuk: `"Try typing a different command name or keyword"` (`theme::TEXT_MUTED`, font `11.5`).

### 3.5 Footer Bar
- Bilah bawah berlatar `theme::CARD`, border atas `theme::BORDER_SUBTLE`, pembulatan sudut bawah 8.0 px.
- Petunjuk interaksi: `"↑↓ Navigate"`, `"·"`, `"↵ Run Command"`, `"·"`, `"Esc Close"` menggunakan font `11.0` warna `theme::TEXT_MUTED`.

---

## 4. Aliran Data & Navigasi Keyboard

1. **Pembukaan**:
   - `Ctrl+Shift+P` atau menu memanggil `self.command_palette.open()`.
   - Mengosongkan query dan me-reset `selected_index = 0`.
2. **Navigasi Panah**:
   - `ArrowDown`: `self.command_palette.move_down(filtered.len())`.
   - `ArrowUp`: `self.command_palette.move_up(filtered.len())`.
3. **Eksekusi & Penutupan**:
   - `Enter` atau klik mouse: Mengambil `cmd.id`, menutup modal (`self.command_palette.close()`), dan mengeksekusi aksi via `self.execute_command(id)`.
   - `Escape` atau klik di luar modal: Menutup modal tanpa eksekusi.

---

## 5. Rencana Verifikasi & Pengujian
1. **Pengujian Unit**: `crates/temp-terminal-ui/tests/command_palette_tests.rs` mencakup pengujian navigasi, clamping, dan filtering.
2. **Kompilasi Workspace**: `cargo check --workspace --all-targets` (0 warning).
3. **Pengujian Suite**: `cargo test -p temp-terminal-ui` (100% lulus).
4. **Build Aplikasi**: `cargo build -p temp-terminal-ui --bin temp-terminal-gui`.
