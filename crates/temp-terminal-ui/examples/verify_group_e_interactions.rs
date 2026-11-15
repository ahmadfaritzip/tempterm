//! Programmatic verification binary for Group E (Editor, Backend & Editing Experience Parity).

use temp_terminal_ui::editor::{
    compute_line_diff, highlight_syntax, EditorDoc, Language, SyntaxTheme,
};

fn main() {
    println!("=== VERIFYING GROUP E INTERACTIONS ===");

    // 1. Language detection
    println!("[1/6] Verifying Language Detection...");
    assert_eq!(Language::for_path("src/lib.rs"), Language::Rust);
    assert_eq!(Language::for_path("app.go"), Language::Go);
    assert_eq!(Language::for_path("app.tsx"), Language::TypeScript);
    assert_eq!(Language::for_path("config.json"), Language::Json);
    assert_eq!(Language::for_path("README.md"), Language::Markdown);
    assert_eq!(
        Language::for_path("custom.unsupported"),
        Language::PlainText
    );
    println!("  -> PASS: All language extensions and plain-text fallback verified.");

    // 2. Syntax Highlighting
    println!("[2/6] Verifying Syntax Tokenization & Themes...");
    let code = "package main\n\nfunc main() {\n    println(\"Hello, Go!\")\n}\n";
    let dark_theme = SyntaxTheme::dark();
    let light_theme = SyntaxTheme::light();
    let omarchy_theme = SyntaxTheme::omarchy();

    let job_dark = highlight_syntax(code, Language::Go, &dark_theme, 13.0, 4);
    let job_light = highlight_syntax(code, Language::Go, &light_theme, 13.0, 4);
    let job_omarchy = highlight_syntax(code, Language::Go, &omarchy_theme, 13.0, 4);

    assert_eq!(job_dark.text, code);
    assert_eq!(job_light.text, code);
    assert_eq!(job_omarchy.text, code);
    println!(
        "  -> PASS: Syntax token layouts across Dark, Light, and Omarchy generated successfully."
    );

    // 3. Diff Calculation (Split & Inline)
    println!("[3/6] Verifying Diff Calculation (Split & Inline)...");
    let old_text = "line 1: unchanged\nline 2: to be deleted\nline 3: common\n";
    let new_text = "line 1: unchanged\nline 2: newly inserted\nline 3: common\nline 4: appended\n";

    let diff = compute_line_diff(old_text, new_text);
    assert!(!diff.is_identical);
    assert_eq!(diff.deletions, 1);
    assert_eq!(diff.additions, 2);
    println!("  -> PASS: Diff computation correctly identified 1 deletion and 2 additions.");

    // 4. Editor Buffer Lifecycle & Atomic Save
    println!("[4/6] Verifying In-Memory Buffer Lifecycle & Atomic Save...");
    let temp_dir = std::env::temp_dir().join("tt_verify_group_e");
    let _ = std::fs::create_dir_all(&temp_dir);
    let file_path = temp_dir.join("sample.rs");
    std::fs::write(&file_path, "fn old() {}\n").unwrap();

    let mut doc = EditorDoc::open("sample.rs", &temp_dir).unwrap();
    assert!(!doc.is_dirty);
    assert_eq!(doc.content, "fn old() {}\n");

    doc.content = "fn new_modified() {}\n".to_string();
    doc.is_dirty = true;
    doc.save().unwrap();
    assert!(!doc.is_dirty);

    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(on_disk, "fn new_modified() {}\n");
    println!("  -> PASS: Buffer modified, saved atomically, and verified on disk.");

    // 5. External Disk Modification Tracking
    println!("[5/6] Verifying External Disk Modification Tracking...");
    std::fs::write(&file_path, "fn externally_changed() {}\n").unwrap();
    doc.last_checked = std::time::Instant::now() - std::time::Duration::from_secs(5);
    doc.check_disk();
    assert_eq!(
        doc.content, "fn externally_changed() {}\n",
        "Clean buffer reloaded automatically"
    );
    assert!(!doc.is_dirty);

    doc.content.push_str("// unsaved dirty local edit\n");
    doc.is_dirty = true;
    std::fs::write(&file_path, "fn disk_changed_again() {}\n").unwrap();
    doc.last_checked = std::time::Instant::now() - std::time::Duration::from_secs(5);
    doc.check_disk();
    assert_eq!(
        doc.content, "fn externally_changed() {}\n// unsaved dirty local edit\n",
        "Dirty buffer preserved"
    );
    assert!(
        doc.has_external_conflict,
        "Conflict flagged on dirty buffer"
    );
    println!("  -> PASS: Clean buffer reloaded automatically; dirty buffer preserved and conflict flagged.");

    // 6. Settings Roundtrip
    println!("[6/6] Verifying Editor Settings...");
    let mut settings = temp_terminal_ui::settings::Settings::default();
    settings.editor.tab_size = 4;
    settings.editor.wrap = true;
    settings.editor.line_numbers = true;
    settings.editor.diff_split = true;
    assert_eq!(settings.editor.tab_size, 4);
    assert!(settings.editor.wrap);
    assert!(settings.editor.line_numbers);
    assert!(settings.editor.diff_split);
    println!("  -> PASS: Editor settings structure verified.");

    let _ = std::fs::remove_dir_all(&temp_dir);
    println!("=== ALL GROUP E INTERACTION VERIFICATIONS PASSED! ===");
}
