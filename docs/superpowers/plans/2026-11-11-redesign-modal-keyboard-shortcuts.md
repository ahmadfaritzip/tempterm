# Implementation Plan: Redesain Modal Keyboard Shortcuts

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merancang ulang modal keyboard shortcuts (`F1` / `Ctrl+Shift+/`) di `temp-terminal-ui` dengan standar Anti-Slop tinggi, kepatuhan token tema terpusat, dan navigasi keyboard yang mulus dilengkapi auto-scroll.

**Architecture:** Memisahkan model data dan logika penyaringan murni ke dalam struktur teruji di `crates/temp-terminal-ui/src/workspace.rs`. Mengganti seluruh hardcoded RGB palette pelangi dengan token tema (`theme::CARD`, `theme::POPOVER`, `theme::SELECTED`, `theme::ACCENT`, `theme::BORDER`), mendesain ulang keycaps individual, serta menambahkan `scroll_to_rect` saat navigasi panah atas/bawah.

**Tech Stack:** Rust, eframe/egui 0.28, tokio.

## Global Constraints

- Sumber Go asli di `omaterm/` bersifat read-only dan tidak boleh diubah.
- Seluruh implementasi baru berada di dalam `temp-terminal/crates/temp-terminal-ui/`.
- DILARANG menggunakan karakter em dash di seluruh antarmuka dan teks bantuan (Anti-Slop R-02).
- Palet aktif dibatasi maksimal 2-3 warna inti netral + 1 warna aksen (Anti-Slop R-29).
- Tidak menggunakan tombol berbentuk pil ekstrem (radius 12+ px), gunakan radius terukur 6.0 px (Anti-Slop R-11).
- Semua warna harus menggunakan token tema terpusat di `theme` agar konsisten lintas tema (Anti-Slop R-34).

---

### Task 1: Data Model & Logika Filter Murni dengan Unit Tests

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs:610-630`
- Create: `crates/temp-terminal-ui/tests/shortcuts_modal_tests.rs`

**Interfaces:**
- Consumes: `ShortcutsOverlayState` dari `workspace.rs`.
- Produces:
  ```rust
  pub struct ShortcutItem {
      pub name: &'static str,
      pub key: &'static str,
      pub category: &'static str,
      pub action_id: &'static str,
  }
  impl ShortcutsOverlayState {
      pub fn open(&mut self);
      pub fn close(&mut self);
      pub fn move_up(&mut self);
      pub fn move_down(&mut self, total_items: usize);
      pub fn set_category_filter(&mut self, category: Option<String>);
  }
  pub fn get_all_shortcuts() -> &'static [ShortcutItem];
  pub fn filter_shortcuts<'a>(shortcuts: &'a [ShortcutItem], query: &str, category: Option<&str>) -> Vec<&'a ShortcutItem>;
  ```

- [ ] **Step 1: Tulis tes unit yang gagal untuk model data dan filter**

Buat berkas `crates/temp-terminal-ui/tests/shortcuts_modal_tests.rs`:
```rust
use temp_terminal_ui::workspace::{filter_shortcuts, get_all_shortcuts, ShortcutsOverlayState, ShortcutItem};

#[test]
fn test_shortcuts_overlay_state_navigation() {
    let mut state = ShortcutsOverlayState::default();
    assert!(!state.is_open);

    state.open();
    assert!(state.is_open);
    assert_eq!(state.selected_index, 0);
    assert!(state.filter.is_empty());

    // Navigation down and up
    state.move_down(5);
    assert_eq!(state.selected_index, 1);
    state.move_down(5);
    assert_eq!(state.selected_index, 2);
    state.move_down(5);
    state.move_down(5);
    state.move_down(5);
    assert_eq!(state.selected_index, 4); // clamped at max (5 - 1)

    state.move_up();
    assert_eq!(state.selected_index, 3);
    state.move_up();
    state.move_up();
    state.move_up();
    state.move_up();
    assert_eq!(state.selected_index, 0); // clamped at 0

    state.close();
    assert!(!state.is_open);
}

#[test]
fn test_filter_shortcuts_query_and_category() {
    let items = [
        ShortcutItem { name: "New Terminal Session", key: "Ctrl+T", category: "Terminal", action_id: "session.new" },
        ShortcutItem { name: "Save Active Editor", key: "Ctrl+S", category: "Editor", action_id: "file.save" },
        ShortcutItem { name: "Toggle Sidebar", key: "Ctrl+B", category: "View", action_id: "view.toggleSidebar" },
    ];

    // Filter by query name
    let res = filter_shortcuts(&items, "Terminal", None);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name, "New Terminal Session");

    // Filter by key combo
    let res = filter_shortcuts(&items, "Ctrl+S", None);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].action_id, "file.save");

    // Filter by category
    let res = filter_shortcuts(&items, "", Some("Editor"));
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name, "Save Active Editor");

    // Filter by non-matching query
    let res = filter_shortcuts(&items, "NonExistentCommand", None);
    assert!(res.is_empty());
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Jalankan: `cargo test -p temp-terminal-ui --test shortcuts_modal_tests`
Ekspektasi: GAGAL karena method `open`, `close`, `move_up`, `move_down`, `get_all_shortcuts`, dan `filter_shortcuts` belum diimplementasikan.

- [ ] **Step 3: Implementasikan struktur data dan helper filter di workspace.rs**

Buka `crates/temp-terminal-ui/src/workspace.rs` dan implementasikan:
```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutItem {
    pub name: &'static str,
    pub key: &'static str,
    pub category: &'static str,
    pub action_id: &'static str,
}

impl ShortcutsOverlayState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.filter.clear();
        self.selected_index = 0;
        self.category_filter = None;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn move_down(&mut self, total_items: usize) {
        if total_items > 0 {
            self.selected_index = (self.selected_index + 1).min(total_items - 1);
        }
    }

    pub fn set_category_filter(&mut self, category: Option<String>) {
        self.category_filter = category;
        self.selected_index = 0;
    }
}

pub fn get_all_shortcuts() -> &'static [ShortcutItem] {
    &[
        ShortcutItem { name: "Command Palette", key: "Ctrl+Shift+P", category: "App", action_id: "app.commandPalette" },
        ShortcutItem { name: "Open Project", key: "Ctrl+O", category: "App", action_id: "app.openProject" },
        ShortcutItem { name: "Go to File (Quick Open)", key: "Ctrl+P", category: "File", action_id: "file.quickOpen" },
        ShortcutItem { name: "Keyboard Shortcuts", key: "F1 / Ctrl+Shift+/", category: "Help", action_id: "help.keyboardShortcuts" },
        ShortcutItem { name: "Settings", key: "Ctrl+,", category: "App", action_id: "app.settings" },
        ShortcutItem { name: "New Terminal Session", key: "Ctrl+T", category: "Session", action_id: "session.new" },
        ShortcutItem { name: "New Agent", key: "F4 / Ctrl+Shift+A", category: "Agent", action_id: "session.newAgent" },
        ShortcutItem { name: "Add Process", key: "F6 / Ctrl+Shift+N", category: "Process", action_id: "process.add" },
        ShortcutItem { name: "Split Terminal Down", key: "Ctrl+Shift+D", category: "Terminal", action_id: "terminal.splitDown" },
        ShortcutItem { name: "Split Terminal Right", key: "Ctrl+Shift+R", category: "Terminal", action_id: "terminal.splitRight" },
        ShortcutItem { name: "Zoom / Maximize Pane", key: "Ctrl+Shift+Z", category: "Terminal", action_id: "pane.zoom" },
        ShortcutItem { name: "Close Terminal Pane / Editor", key: "Ctrl+W", category: "Terminal", action_id: "terminal.closePane" },
        ShortcutItem { name: "Close Active Split Pane", key: "Ctrl+Shift+W", category: "Terminal", action_id: "terminal.closePane" },
        ShortcutItem { name: "Cycle Pane Focus Forward", key: "Ctrl+Tab", category: "Terminal", action_id: "terminal.cycleForward" },
        ShortcutItem { name: "Cycle Pane Focus Reverse", key: "Ctrl+Shift+Tab", category: "Terminal", action_id: "terminal.cycleReverse" },
        ShortcutItem { name: "Copy Selected Text", key: "Ctrl+C", category: "Terminal", action_id: "terminal.copy" },
        ShortcutItem { name: "Paste from Clipboard", key: "Ctrl+V / Shift+Ins", category: "Terminal", action_id: "terminal.paste" },
        ShortcutItem { name: "Delete Word Backward", key: "Ctrl+BS / Alt+BS", category: "Terminal", action_id: "terminal.delWord" },
        ShortcutItem { name: "Save Active Editor", key: "Ctrl+S", category: "Editor", action_id: "file.save" },
        ShortcutItem { name: "Toggle Diff View", key: "F7 / Ctrl+D", category: "Editor", action_id: "editor.toggleDiff" },
        ShortcutItem { name: "Cycle Workspace Files", key: "F8", category: "Editor", action_id: "editor.cycleFiles" },
        ShortcutItem { name: "Toggle Left Sidebar", key: "Ctrl+B", category: "View", action_id: "view.toggleSidebar" },
        ShortcutItem { name: "Toggle Right Inspector", key: "Ctrl+Shift+B", category: "View", action_id: "view.toggleInspector" },
        ShortcutItem { name: "Inspector Info Tab", key: "Ctrl+Shift+I", category: "View", action_id: "view.info" },
        ShortcutItem { name: "Inspector Files Tab", key: "Ctrl+Shift+E", category: "View", action_id: "view.files" },
        ShortcutItem { name: "Inspector Git Tab", key: "Ctrl+Shift+G", category: "View", action_id: "view.git" },
        ShortcutItem { name: "Cycle Theme", key: "F9", category: "View", action_id: "app.cycleTheme" },
    ]
}

pub fn filter_shortcuts<'a>(
    shortcuts: &'a [ShortcutItem],
    query: &str,
    category: Option<&str>,
) -> Vec<&'a ShortcutItem> {
    let q = query.trim().to_lowercase();
    shortcuts
        .iter()
        .filter(|s| {
            if let Some(cf) = category {
                if cf != "All" {
                    let match_cat = match cf {
                        "Terminal" => s.category == "Terminal" || s.category == "Session",
                        "App" => s.category == "App" || s.category == "File" || s.category == "Help",
                        "Editor" => s.category == "Editor",
                        "View" => s.category == "View",
                        "Agent" => s.category == "Agent" || s.category == "Process",
                        _ => s.category == cf,
                    };
                    if !match_cat {
                        return false;
                    }
                }
            }
            if q.is_empty() {
                true
            } else {
                s.name.to_lowercase().contains(&q)
                    || s.key.to_lowercase().contains(&q)
                    || s.category.to_lowercase().contains(&q)
            }
        })
        .collect()
}
```

- [ ] **Step 4: Jalankan tes unit untuk memastikan lulus**

Jalankan: `cargo test -p temp-terminal-ui --test shortcuts_modal_tests`
Ekspektasi: LULUS (semua tes hijau).

- [ ] **Step 5: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs crates/temp-terminal-ui/tests/shortcuts_modal_tests.rs
git commit -m "feat(ui): add data model and pure filtering logic for keyboard shortcuts modal"
```

---

### Task 2: Redesain Header, Bilah Pencarian, & Chip Kategori dengan Token Tema

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `theme::POPOVER`, `theme::BORDER_MEDIUM`, `theme::BORDER_SUBTLE`, `theme::TEXT_STRONG`, `theme::TEXT_MUTED`, `theme::ACCENT`, `theme::SELECTED`, `theme::CARD`.
- Produces: Rendering header bersih, search field auto-focus, dan category chips bersudut terukur (6.0 px).

- [ ] **Step 1: Terapkan framing modal dan Header Bar sesuai token tema**

Perbarui blok inisialisasi `egui::Window` untuk `keyboard_shortcuts_modal`:
- Ukuran: `fixed_size(Vec2::new(740.0, 540.0))`
- Frame:
  - Fill: `theme::SURFACE_OVERLAY` (`theme::POPOVER`)
  - Stroke: `Stroke::new(1.0, theme::BORDER_MEDIUM)`
  - Rounding: `Rounding::same(8.0)`
- Header:
  - Ikon: `Icons::draw_keyboard(ui.painter(), icon_r, theme::ACCENT)`
  - Judul: `"Keyboard Shortcuts"` dengan `theme::TEXT_STRONG`
  - Tombol tutup (`×`): Menggunakan warna `theme::TEXT_MUTED` dengan hover state bersih dan kursor pointer.

- [ ] **Step 2: Redesain Search Input & Filter Chips**

- Input pencarian:
  - Fill: `theme::SURFACE_INPUT` (`theme::CARD`)
  - Border: `Stroke::new(1.0, theme::BORDER_MEDIUM)`
  - Rounding: `Rounding::same(6.0)` (bukan sudut pil)
  - Auto-request focus jika belum memiliki fokus
- Chip kategori:
  - Kategori: `[("All", None), ("Terminal", Some("Terminal")), ("App", Some("App")), ("Editor", Some("Editor")), ("View", Some("View")), ("Agent", Some("Agent"))]`
  - Tombol chip menggunakan `Rounding::same(6.0)`
  - Chip aktif: Fill `theme::SELECTED`, stroke `Stroke::new(1.0, theme::ACCENT)`, teks `Color32::WHITE`
  - Chip tidak aktif: Fill `theme::CARD`, stroke `Stroke::new(1.0, theme::BORDER_SUBTLE)`, teks `theme::TEXT_MUTED`

- [ ] **Step 3: Verifikasi kompilasi**

Jalankan: `cargo check -p temp-terminal-ui`
Ekspektasi: LULUS tanpa error kompilasi.

- [ ] **Step 4: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "refactor(ui): update shortcuts modal header, search, and category chips to theme tokens"
```

---

### Task 3: Redesain Baris Tabel, Badge Kategori Netral, & Keycaps Individual

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: `filter_shortcuts`, `theme` tokens.
- Produces: Rendering baris shortcut dengan keycaps individual berstandar Anti-Slop (tanpa warna pelangi).

- [ ] **Step 1: Hapus skema warna pelangi kategori hardcoded**

Hapus blok `match item.category` dengan warna RGB pelangi (`0x17, 0x25, 0x54`, `0x2e, 0x10, 0x65`, dll.) dan gantikan dengan badge kategori netral:
```rust
let cat_bg = if is_sel { theme::SURFACE_OVERLAY } else { theme::SURFACE_CARD };
let cat_stroke = if is_sel { theme::ACCENT } else { theme::BORDER_SUBTLE };
let cat_text = if is_sel { Color32::WHITE } else { theme::TEXT_MUTED };
```

- [ ] **Step 2: Redesain rendering keycaps individual**

Format tombol kombinasi (`parts = item.key.split(" / ")`, `sub_keys = combo.split('+')`):
- Keycap kotak terpisah dengan font monospace `10.5`
- Keycap normal:
  - Fill: `theme::CARD`
  - Stroke: `Stroke::new(1.0, theme::BORDER_STRONG)`
  - Text: `theme::TEXT`
- Keycap saat baris terpilih:
  - Fill: `theme::ACCENT`
  - Stroke: `Stroke::new(1.0, theme::ACCENT_HOVER)`
  - Text: `Color32::WHITE`
- Rounding: `Rounding::same(4.0)`

- [ ] **Step 3: Redesain Footer Bar**

Footer terpasang di bagian bawah modal:
- Fill: `theme::CARD`
- Stroke: `Stroke::new(1.0, theme::BORDER_SUBTLE)`
- Rounding: `Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 8.0 }`
- Teks navigasi: `"↑↓ Navigasi"`, `"·"`, `"↵ Jalankan Perintah"`, `"·"`, `"Esc Tutup"` menggunakan font proporsional dan `theme::TEXT_MUTED`.

- [ ] **Step 4: Verifikasi kompilasi**

Jalankan: `cargo check -p temp-terminal-ui`
Ekspektasi: LULUS.

- [ ] **Step 5: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "feat(ui): eliminate rainbow badge slop and style individual keycaps with theme tokens"
```

---

### Task 4: Implementasi Auto-Scroll & Navigasi Keyboard Penuh

**Files:**
- Modify: `crates/temp-terminal-ui/src/workspace.rs`

**Interfaces:**
- Consumes: Input event `ArrowUp`, `ArrowDown`, `Enter`, `Escape`.
- Produces: Respons navigasi keyboard dengan `ui.scroll_to_rect(row_rect, Some(egui::Align::Center))` saat item terpilih berganti.

- [ ] **Step 1: Terapkan auto-scroll pada baris yang aktif**

Di dalam loop perenderan `ScrollArea`:
```rust
if is_sel {
    ui.scroll_to_rect(row_rect, Some(egui::Align::Center));
}
```

- [ ] **Step 2: Tangani eksekusi Enter dan Escape secara aman**

Pastikan menekan `Enter` saat `filtered` tidak kosong:
1. Mengambil `item = filtered[selected_index]`
2. Mengeset `command_to_execute = Some(item.action_id)`
3. Mengeset `self.shortcuts_overlay.close()`
4. Memanggil `self.execute_command(cmd)` di luar blok window modal.

- [ ] **Step 3: Verifikasi kompilasi dan tes**

Jalankan: `cargo test -p temp-terminal-ui`
Ekspektasi: LULUS.

- [ ] **Step 4: Commit**

```bash
git add crates/temp-terminal-ui/src/workspace.rs
git commit -m "feat(ui): implement auto-scroll and seamless keyboard navigation for shortcuts modal"
```

---

### Task 5: Validasi Menyeluruh & Audit Anti-Slop Akhir

**Files:**
- Test: Seluruh unit test dan integrasi di workspace.

- [ ] **Step 1: Jalankan kompilasi penuh seluruh workspace**

Jalankan: `cargo check --workspace`
Ekspektasi: Kompilasi berhasil tanpa peringatan error.

- [ ] **Step 2: Jalankan seluruh rangkaian tes unit dan integrasi**

Jalankan: `cargo test -p temp-terminal-ui`
Ekspektasi: Seluruh tes lulus (100% pass).

- [ ] **Step 3: Audit checklist Anti-Slop (Delivery Gate)**

Verifikasi:
- R-01 & R-29: Tidak ada warna pelangi acak. Palet bersih dengan 1 aksen.
- R-02: Tidak ada karakter em dash (`—`).
- R-11: Tidak ada tombol pil radius 12.0 px yang berlebihan.
- R-25: Kontras teks memenuhi standar WCAG AA.
- R-27: Terdapat empty state informatif jika pencarian kosong.
- R-32 & R-39: Navigasi keyboard penuh, kursor pointer, dan hover feedback.
- R-34: Modal kompatibel dan responsif terhadap token tema.

- [ ] **Step 4: Commit dokumentasi final**

```bash
git add docs/superpowers/plans/2026-11-11-redesign-modal-keyboard-shortcuts.md
git commit -m "docs: finalize implementation plan for keyboard shortcuts modal redesign"
```
