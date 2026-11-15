# Spesifikasi Desain: Redesain Modal Keyboard Shortcuts

- **Tanggal**: 2026-11-11
- **Status**: Disetujui
- **Komponen Target**: `crates/temp-terminal-ui` (`workspace.rs`, `ShortcutsOverlayState`)

---

## 1. Latar Belakang & Tujuan
Modal Keyboard Shortcuts (`F1` atau `Ctrl+Shift+/`) saat ini memiliki beberapa masalah desain dan ergonomi:
1. Menggunakan warna RGB mentah yang di-hardcode (`Color32::from_rgb(...)`), mengabaikan token tema terpusat, dan berpotensi rusak pada tema non-gelap.
2. Memakai palet warna pelangi untuk kategori (biru, ungu, hijau, cokelat, dll.) yang melanggar aturan Anti-Slop R-29 (maksimal 2-3 warna inti + 1 aksen).
3. Menggunakan tombol filter berbentuk kapsul berlebihan (pill buttons radius 12.0 px) yang melanggar aturan Anti-Slop R-11.
4. Navigasi panah atas/bawah keyboard belum memiliki mekanisme auto-scroll, sehingga baris aktif dapat keluar dari area yang terlihat pada layar.
5. Koordinat header tabel dihitung secara manual dengan angka absolut yang rapuh terhadap penskalaan font.

Tujuan perancangan ulang ini adalah menghadirkan modal keyboard shortcuts yang bersih, berstandar Anti-Slop tinggi, responsif terhadap tema, dan berorientasi pada kecepatan navigasi keyboard (keyboard-first).

---

## 2. Arsitektur & Struktur State

### 2.1 State Model
Model state dipertahankan di dalam `WorkspaceApp`:
```rust
#[derive(Clone, Debug, Default)]
pub struct ShortcutsOverlayState {
    pub is_open: bool,
    pub filter: String,
    pub selected_index: usize,
    pub category_filter: Option<String>,
}
```

### 2.2 Model Data Shortcut
Data shortcut terstruktur rapi dengan informasi nama, tombol, kategori, dan ID aksi yang dapat dieksekusi:
```rust
struct ShortcutItem {
    name: &'static str,
    key: &'static str,
    category: &'static str,
    action_id: &'static str,
}
```

---

## 3. Desain Komponen UI & Token Tema

### 3.1 Penataan Modal
Modal menggunakan `egui::Window` berukuran tetap (740 x 540 px) yang diposisikan di tengah layar (`CENTER_CENTER`):
- **Frame & Permukaan**: Latar `theme::SURFACE_OVERLAY` (`theme::POPOVER`), stroke `theme::BORDER_MEDIUM` 1.0 px, sudut pembulatan terukur 8.0 px (konsisten dengan dialog lain).
- **Bayangan (Elevation)**: Bayangan halus satu lapis untuk memisahkan modal dari kanvas terminal.

### 3.2 Header Bar
- Ikon keyboard (`Icons::draw_keyboard`) dengan warna aksen `theme::ACCENT`.
- Teks judul `"Keyboard Shortcuts"` (`theme::TEXT_STRONG`).
- Badge keycap pembuka (`F1 / Ctrl+Shift+/`) di sisi kanan dengan border `theme::BORDER_STRONG` dan font monospace.
- Tombol tutup (`×`) interaktif dengan hover state bersih dan kursor pointer.

### 3.3 Bilah Pencarian & Chip Filter
- **Input Pencarian**:
  - Kolom teks tunggal terintegrasi dengan ikon pencarian `Icons::draw_search`.
  - Placeholder: `"Cari pintasan atau tombol kombinasi..."`.
  - Auto-focus saat modal dibuka.
  - Teks jumlah hasil pencarian di sisi kanan (misal: `"27 commands"`).
- **Filter Kategori (Chips)**:
  - Opsi: `Semua`, `Terminal`, `App`, `Editor`, `View`, `Agent`.
  - Radius pembulatan terukur (6.0 px, bukan pill 12+ px).
  - Chip aktif: Latar `theme::SELECTED`, border `theme::ACCENT`, teks `theme::TEXT_STRONG`.
  - Chip tidak aktif: Latar `theme::CARD`, border `theme::BORDER_SUBTLE`, teks `theme::TEXT_MUTED`.

### 3.4 Tabel Daftar Perintah
- **Header Kolom**:
  - Judul kolom: `PERINTAH`, `KATEGORI`, `TOMBOL PINTASAN`.
  - Menggunakan tata letak horizontal egui yang fleksibel tanpa hardcoded offset absolut.
- **Baris Item**:
  - Tinggi baris 34 px dengan margin horizontal 12 px dan vertikal 2 px.
  - Status hover: Latar `theme::HOVER`.
  - Status terpilih (`is_sel`): Latar `theme::SELECTED` dengan stroke `theme::ACCENT`.
  - **Auto-scroll**: Saat baris dipilih via keyboard panah atas/bawah, panggil `ui.scroll_to_rect(row_rect, Some(egui::Align::Center))` agar baris aktif selalu berada di area pandang.
- **Badge Kategori**:
  - Menggunakan palet monokrom/netral: Latar `theme::CARD`, stroke `theme::BORDER_SUBTLE`, teks `theme::TEXT_MUTED`.
  - Tidak ada lagi warna pelangi (kuning, ungu, hijau) per kategori.
- **Keycaps Tombol**:
  - Setiap tombol dalam kombinasi (misal: `Ctrl`, `Shift`, `P`) dirender dalam kotak keycap terpisah.
  - Keycap normal: Latar `theme::CARD`, border `theme::BORDER_STRONG`, teks monospace `theme::TEXT`.
  - Keycap terpilih: Latar `theme::ACCENT`, border `theme::ACCENT_HOVER`, teks putih kontras tinggi.

### 3.5 State Kosong (Empty State)
- Jika pencarian tidak menemukan hasil, tampilkan pesan informatif yang jelas:
  - Judul: `"Tidak ada pintasan yang cocok"` (`theme::TEXT_FAINT`).
  - Keterangan: `"Coba cari dengan nama perintah lain atau ubah filter kategori."` (`theme::TEXT_MUTED`).

### 3.6 Footer Bar
- Bilah penutup di bagian bawah modal dengan pembulatan sudut bawah (8.0 px).
- Petunjuk interaksi: `"↑↓ Navigasi"`, `"↵ Jalankan Perintah"`, `"Esc Tutup"`.
- Warna latar `theme::CARD` dengan stroke atas `theme::BORDER_SUBTLE`.

---

## 4. Aliran Data & Navigasi Keyboard

1. **Pembukaan**:
   - Menekan `F1` atau `Ctrl+Shift+/` membalikkan nilai `shortcuts_overlay.is_open`.
   - Jika terbuka, reset `filter = ""`, `selected_index = 0`, dan `category_filter = None`.
2. **Penyaringan**:
   - Filter query dicocokkan terhadap nama, key kombinasi, dan kategori.
   - Filter kategori mempersempit kumpulan item.
3. **Interaksi Tombol**:
   - `ArrowDown`: `selected_index = (selected_index + 1).min(filtered.len().saturating_sub(1))`
   - `ArrowUp`: `selected_index = selected_index.saturating_sub(1)`
   - `Enter`: Eksekusi perintah yang dipilih via `execute_command(item.action_id)` dan tutup modal.
   - `Escape`: Tutup modal tanpa eksekusi.
4. **Interaksi Klik Mouse**:
   - Mengklik baris shortcut langsung mengeksekusi aksi dan menutup modal.
   - Mengklik chip kategori memperbarui `category_filter` dan me-reset `selected_index = 0`.

---

## 5. Kepatuhan Anti-Slop (Audit Checklist)
- [x] **R-01 & R-29 (Warna & Palet)**: Menghapus 5+ skema warna kategori pelangi. Membatasi palet pada warna netral tema (`theme::CARD`, `theme::POPOVER`, `theme::HOVER`, `theme::SELECTED`) dan 1 aksen (`theme::ACCENT`).
- [x] **R-02 (Copywriting)**: Tidak ada karakter em dash di seluruh antarmuka dan teks bantuan.
- [x] **R-11 (Border Radius)**: Menghapus bentuk pil ekstrem (radius 12.0 px) pada chip tombol, diganti dengan radius terkontrol (6.0 px).
- [x] **R-25 (Kontras Warna)**: Memastikan rasio kontras teks utama dan keycaps memenuhi standar WCAG AA.
- [x] **R-27 (UI States)**: Menyediakan empty state informatif yang menjelaskan penyebab dan aksi perbaikan saat pencarian tidak menemukan hasil.
- [x] **R-32 & R-39 (Aksesibilitas Keyboard & Kursor)**: Navigasi keyboard penuh dengan auto-scroll dan penanda kursor interaktif.
- [x] **R-34 (Dukungan Tema)**: Semua warna mengacu pada token tema dinamis sehingga tampilan konsisten di tema Dark, Light, maupun Omarchy.

---

## 6. Rencana Verifikasi & Pengujian
1. **Pemeriksaan Kompilasi**: Jalankan `cargo check -p temp-terminal-ui` untuk memastikan bebas kesalahan sintaks dan tipe data.
2. **Pengujian Unit**: Jalankan `cargo test -p temp-terminal-ui` untuk memastikan tes yang ada tetap lulus.
3. **Pemeriksaan Visual**: Memastikan tata letak bersih, keycaps terpisah rapi, filter kategori berfungsi, dan auto-scroll berjalan mulus saat berpindah baris.
