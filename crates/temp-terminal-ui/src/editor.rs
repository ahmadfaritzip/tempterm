//! Editor buffer representation, syntax highlighting, diff viewing, and editing lifecycle.
//!
//! Matches original Go `tempterm/editor.go`, `tempterm/diff.go`, and
//! `tempterm/internal/theme/term.go`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, FontId, TextFormat};

/// Maximum file size the editor will open (32 MB), matching `tempterm/editor.go:247` (`maxEditorSize = 32 << 20`).
pub const MAX_EDITOR_SIZE: u64 = 32 * 1024 * 1024;

/// Supported languages for syntax tokenization and highlighting.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    Go,
    TypeScript,
    JavaScript,
    Json,
    Yaml,
    Toml,
    Markdown,
    Python,
    HtmlXml,
    Css,
    Shell,
    PowerShell,
    C,
    Cpp,
    PlainText,
}

impl Language {
    /// Detects language from file path extension or file name.
    pub fn for_path(path: &str) -> Self {
        let p = path.to_lowercase();
        let ext = p.rsplit('.').next().unwrap_or("");
        let file_name = p.rsplit(&['/', '\\'][..]).next().unwrap_or(&p);

        if file_name == "dockerfile" || file_name.ends_with(".dockerfile") {
            return Language::Shell;
        }
        if file_name == "cargo.lock" || file_name == "cargo.toml" {
            return Language::Toml;
        }

        match ext {
            "rs" => Language::Rust,
            "go" => Language::Go,
            "ts" | "tsx" => Language::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "json" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "md" | "mdx" => Language::Markdown,
            "py" => Language::Python,
            "html" | "htm" | "xml" | "svg" => Language::HtmlXml,
            "css" | "scss" | "sass" | "less" => Language::Css,
            "sh" | "bash" | "zsh" => Language::Shell,
            "ps1" | "psm1" | "psd1" => Language::PowerShell,
            "c" | "h" => Language::C,
            "cpp" | "cxx" | "cc" | "hpp" => Language::Cpp,
            _ => Language::PlainText,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::Python => "Python",
            Language::HtmlXml => "HTML",
            Language::Css => "CSS",
            Language::Shell => "Shell",
            Language::PowerShell => "PowerShell",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::PlainText => "Plain Text",
        }
    }
}

/// Token classification matching `tempterm/internal/theme/term.go:40-90`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    Type,
    Function,
    Variable,
    Number,
    String,
    Comment,
    Operator,
    Punctuation,
    Tag,
    Attribute,
    Heading,
    Text,
}

/// Token color palette for syntax rendering.
#[derive(Clone, Debug)]
pub struct SyntaxTheme {
    pub keyword: Color32,
    pub type_name: Color32,
    pub function: Color32,
    pub variable: Color32,
    pub number: Color32,
    pub string: Color32,
    pub comment: Color32,
    pub punctuation: Color32,
    pub tag: Color32,
    pub attribute: Color32,
    pub line_number: Color32,
    pub active_line_number: Color32,
    pub diff_add_bg: Color32,
    pub diff_add_fg: Color32,
    pub diff_del_bg: Color32,
    pub diff_del_fg: Color32,
}

impl SyntaxTheme {
    /// Dark theme palette matching `tempterm/internal/theme/builtin.go:12-60`.
    pub fn dark() -> Self {
        Self {
            keyword: Color32::from_rgb(0xc6, 0x78, 0xdd), // #c678dd purple
            type_name: Color32::from_rgb(0xe5, 0xc0, 0x7b), // #e5c07b amber
            function: Color32::from_rgb(0x61, 0xaf, 0xef), // #61afef blue
            variable: Color32::from_rgb(0xe0, 0x6c, 0x75), // #e06c75 soft red
            number: Color32::from_rgb(0xd1, 0x9a, 0x66),  // #d19a66 orange
            string: Color32::from_rgb(0x98, 0xc3, 0x79),  // #98c379 green
            comment: Color32::from_rgb(0x7f, 0x84, 0x8e), // #7f848e muted gray
            punctuation: Color32::from_rgb(0xab, 0xb2, 0xbf), // #abb2bf light gray
            tag: Color32::from_rgb(0xe0, 0x6c, 0x75),     // #e06c75
            attribute: Color32::from_rgb(0xd1, 0x9a, 0x66), // #d19a66
            line_number: Color32::from_rgb(0x4d, 0x56, 0x63), // #4d5663
            active_line_number: Color32::from_rgb(0xff, 0xff, 0xff),
            diff_add_bg: Color32::from_rgba_premultiplied(63, 185, 80, 38), // rgba(63, 185, 80, 0.15)
            diff_add_fg: Color32::from_rgb(0x63, 0xd5, 0x8d),
            diff_del_bg: Color32::from_rgba_premultiplied(248, 81, 73, 38), // rgba(248, 81, 73, 0.15)
            diff_del_fg: Color32::from_rgb(0xff, 0x6f, 0x6f),
        }
    }

    /// Light theme palette.
    pub fn light() -> Self {
        Self {
            keyword: Color32::from_rgb(0xa6, 0x26, 0xa4),
            type_name: Color32::from_rgb(0xc1, 0x84, 0x01),
            function: Color32::from_rgb(0x40, 0x78, 0xf2),
            variable: Color32::from_rgb(0xe4, 0x56, 0x49),
            number: Color32::from_rgb(0x98, 0x68, 0x01),
            string: Color32::from_rgb(0x50, 0xa1, 0x4f),
            comment: Color32::from_rgb(0xa0, 0xa1, 0xa7),
            punctuation: Color32::from_rgb(0x38, 0x3a, 0x42),
            tag: Color32::from_rgb(0xe4, 0x56, 0x49),
            attribute: Color32::from_rgb(0x98, 0x68, 0x01),
            line_number: Color32::from_rgb(0x9d, 0xa5, 0xb4),
            active_line_number: Color32::from_rgb(0x11, 0x16, 0x1c),
            diff_add_bg: Color32::from_rgba_premultiplied(46, 160, 67, 38),
            diff_add_fg: Color32::from_rgb(0x1a, 0x7f, 0x37),
            diff_del_bg: Color32::from_rgba_premultiplied(207, 34, 46, 38),
            diff_del_fg: Color32::from_rgb(0xcf, 0x22, 0x2e),
        }
    }

    /// Omarchy theme palette (amber/monochrome accent).
    pub fn omarchy() -> Self {
        Self {
            keyword: Color32::from_rgb(0xff, 0xb4, 0x54),
            type_name: Color32::from_rgb(0xff, 0xcc, 0x66),
            function: Color32::from_rgb(0x59, 0xc2, 0xc6),
            variable: Color32::from_rgb(0xf0, 0x71, 0x78),
            number: Color32::from_rgb(0xff, 0xb4, 0x54),
            string: Color32::from_rgb(0xc2, 0xd9, 0x4c),
            comment: Color32::from_rgb(0x62, 0x6a, 0x73),
            punctuation: Color32::from_rgb(0xbf, 0xba, 0xb0),
            tag: Color32::from_rgb(0xf0, 0x71, 0x78),
            attribute: Color32::from_rgb(0xff, 0xb4, 0x54),
            line_number: Color32::from_rgb(0x62, 0x6a, 0x73),
            active_line_number: Color32::from_rgb(0xff, 0xb4, 0x54),
            diff_add_bg: Color32::from_rgba_premultiplied(194, 217, 76, 38),
            diff_add_fg: Color32::from_rgb(0xc2, 0xd9, 0x4c),
            diff_del_bg: Color32::from_rgba_premultiplied(240, 113, 120, 38),
            diff_del_fg: Color32::from_rgb(0xf0, 0x71, 0x78),
        }
    }
}

/// In-memory representation of an open editor file matching Go `openFile` in `tempterm/editor.go:34-45`.
#[derive(Clone, Debug)]
pub struct EditorDoc {
    pub path: String,
    pub full_path: PathBuf,
    pub title: String,
    pub content: String,
    pub disk_hash: [u8; 32],
    pub is_dirty: bool,
    pub is_binary: bool,
    pub is_too_large: bool,
    pub size: u64,
    pub last_checked: Instant,
    pub has_external_conflict: bool,
    pub language: Language,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub is_preview: bool,
}

impl EditorDoc {
    /// Opens a file from disk into an `EditorDoc`, enforcing size limits and binary detection.
    pub fn open(rel_path: &str, base_dir: &Path) -> Result<Self, std::io::Error> {
        let full_path = if Path::new(rel_path).is_absolute() {
            PathBuf::from(rel_path)
        } else {
            base_dir.join(rel_path)
        };

        let file_name = full_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_path.to_string());

        let metadata = std::fs::metadata(&full_path)?;
        let size = metadata.len();

        if size > MAX_EDITOR_SIZE {
            return Ok(Self {
                path: rel_path.to_string(),
                full_path,
                title: file_name,
                content: format!(
                    "{} · too large to open here: use an editor of your own",
                    format_bytes(size)
                ),
                disk_hash: [0u8; 32],
                is_dirty: false,
                is_binary: true,
                is_too_large: true,
                size,
                last_checked: Instant::now(),
                has_external_conflict: false,
                language: Language::PlainText,
                cursor_line: 0,
                cursor_col: 0,
                is_preview: false,
            });
        }

        let bytes = std::fs::read(&full_path)?;
        let disk_hash = temp_terminal_procs::def::sha256_digest(&bytes);

        if is_binary_data(&bytes) {
            return Ok(Self {
                path: rel_path.to_string(),
                full_path,
                title: file_name,
                content: format!(
                    "Binary file · {} · preview not available",
                    format_bytes(size)
                ),
                disk_hash,
                is_dirty: false,
                is_binary: true,
                is_too_large: false,
                size,
                last_checked: Instant::now(),
                has_external_conflict: false,
                language: Language::PlainText,
                cursor_line: 0,
                cursor_col: 0,
                is_preview: false,
            });
        }

        let content = String::from_utf8_lossy(&bytes).to_string();
        let language = Language::for_path(rel_path);

        Ok(Self {
            path: rel_path.to_string(),
            full_path,
            title: file_name,
            content,
            disk_hash,
            is_dirty: false,
            is_binary: false,
            is_too_large: false,
            size,
            last_checked: Instant::now(),
            has_external_conflict: false,
            language,
            cursor_line: 0,
            cursor_col: 0,
            is_preview: false,
        })
    }

    /// Constructs an in-memory test or virtual document.
    pub fn new_memory(path: &str, title: &str, content: &str) -> Self {
        let bytes = content.as_bytes();
        let disk_hash = temp_terminal_procs::def::sha256_digest(bytes);
        let language = Language::for_path(path);
        Self {
            path: path.to_string(),
            full_path: PathBuf::from(path),
            title: title.to_string(),
            content: content.to_string(),
            disk_hash,
            is_dirty: false,
            is_binary: false,
            is_too_large: false,
            size: bytes.len() as u64,
            last_checked: Instant::now(),
            has_external_conflict: false,
            language,
            cursor_line: 0,
            cursor_col: 0,
            is_preview: false,
        }
    }

    /// Saves buffer content back to disk atomically.
    pub fn save(&mut self) -> Result<(), std::io::Error> {
        if self.is_binary || self.is_too_large {
            return Ok(());
        }

        write_file_atomic(&self.full_path, self.content.as_bytes())?;
        self.disk_hash = temp_terminal_procs::def::sha256_digest(self.content.as_bytes());
        self.size = self.content.len() as u64;
        self.is_dirty = false;
        self.has_external_conflict = false;
        self.last_checked = Instant::now();
        Ok(())
    }

    /// Checks if file was modified externally on disk (Go `checkDisk` in `tempterm/editor.go:336`).
    pub fn check_disk(&mut self) {
        if self.is_binary || self.is_too_large {
            return;
        }

        if self.last_checked.elapsed().as_secs() < 2 {
            return;
        }
        self.last_checked = Instant::now();

        if let Ok(data) = std::fs::read(&self.full_path) {
            let sum = temp_terminal_procs::def::sha256_digest(&data);
            if sum != self.disk_hash {
                if !self.is_dirty {
                    // Buffer is clean: reload automatically
                    self.content = String::from_utf8_lossy(&data).to_string();
                    self.disk_hash = sum;
                    self.size = data.len() as u64;
                    self.has_external_conflict = false;
                } else {
                    // Buffer is dirty: flag external conflict
                    self.has_external_conflict = true;
                }
            }
        }
    }
}

/// Checks whether data has null bytes in the first 1024 bytes (Go `service.IsBinary`).
pub fn is_binary_data(data: &[u8]) -> bool {
    let check_len = data.len().min(1024);
    data[..check_len].contains(&0)
}

/// Human-readable byte formatting matching Go `service.FormatBytes`.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Atomic file replacement with Windows sharing retry matching Go `writeFileAtomic` in `tempterm/editor.go:273`.
pub fn write_file_atomic(path: &Path, data: &[u8]) -> Result<(), std::io::Error> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("editor");

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let cnt = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = dir.join(format!(".{}.{}_{}_{}.tmp", file_name, pid, nonce, cnt));

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_path)?;

    file.write_all(data)?;
    file.sync_all()?;
    drop(file);

    let rename_res = atomic_rename(&tmp_path, path);
    if rename_res.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    rename_res
}

/// Windows sharing-tolerant atomic rename matching Go `atomicfile.Rename` in `tempterm/internal/atomicfile/rename_windows.go`.
fn atomic_rename(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
    #[cfg(windows)]
    {
        let mut attempts = 0;
        let max_attempts = 30;
        let mut sleep_ms = 10;

        loop {
            match std::fs::rename(from, to) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err(e);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                    sleep_ms = (sleep_ms * 2).min(100);
                }
            }
        }
    }
}

/// Tokenizes source code into an `egui::text::LayoutJob` with appropriate styling.
pub fn highlight_syntax(
    text: &str,
    lang: Language,
    theme: &SyntaxTheme,
    font_size: f32,
    tab_size: usize,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font_id = FontId::monospace(font_size);

    if lang == Language::PlainText {
        job.append(
            text,
            0.0,
            TextFormat {
                font_id,
                color: theme.punctuation,
                ..Default::default()
            },
        );
        return job;
    }

    let tokens = tokenize(text, lang);
    for (token_text, kind) in tokens {
        let (color, italics, bold) = match kind {
            TokenKind::Keyword => (theme.keyword, false, true),
            TokenKind::Type => (theme.type_name, false, false),
            TokenKind::Function => (theme.function, false, false),
            TokenKind::Variable => (theme.variable, false, false),
            TokenKind::Number => (theme.number, false, false),
            TokenKind::String => (theme.string, false, false),
            TokenKind::Comment => (theme.comment, true, false),
            TokenKind::Operator | TokenKind::Punctuation => (theme.punctuation, false, false),
            TokenKind::Tag => (theme.tag, false, false),
            TokenKind::Attribute => (theme.attribute, false, false),
            TokenKind::Heading => (theme.function, false, true),
            TokenKind::Text => (theme.punctuation, false, false),
        };

        let mut format = TextFormat {
            font_id: font_id.clone(),
            color,
            italics,
            ..Default::default()
        };
        if bold {
            format.font_id = FontId::monospace(font_size);
        }

        // Handle tab expansion
        if token_text.contains('\t') {
            let expanded = token_text.replace('\t', &" ".repeat(tab_size.max(1)));
            job.append(&expanded, 0.0, format);
        } else {
            job.append(&token_text, 0.0, format);
        }
    }

    job
}

/// Fast lexical tokenizer for supported languages.
pub fn tokenize(text: &str, lang: Language) -> Vec<(String, TokenKind)> {
    let mut result = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    let keywords = get_keywords(lang);
    let types = get_types(lang);

    while i < len {
        let c = chars[i];

        // 1. Line comments
        if (c == '/' && i + 1 < len && chars[i + 1] == '/')
            || (c == '#'
                && (lang == Language::Python
                    || lang == Language::Shell
                    || lang == Language::Yaml
                    || lang == Language::Toml))
        {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            result.push((s, TokenKind::Comment));
            continue;
        }

        // 2. Block comments (/* ... */)
        if c == '/' && i + 1 < len && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            let s: String = chars[start..i].iter().collect();
            result.push((s, TokenKind::Comment));
            continue;
        }

        // 3. Strings
        if c == '"'
            || c == '\''
            || (c == '`'
                && (lang == Language::Go
                    || lang == Language::JavaScript
                    || lang == Language::TypeScript
                    || lang == Language::Markdown))
        {
            let quote = c;
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < len {
                if escaped {
                    escaped = false;
                } else if chars[i] == '\\' && quote != '`' {
                    escaped = true;
                } else if chars[i] == quote {
                    i += 1;
                    break;
                } else if chars[i] == '\n' && quote != '`' {
                    break;
                }
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            result.push((s, TokenKind::String));
            continue;
        }

        // 4. Numbers
        if c.is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_')
        {
            let start = i;
            while i < len
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
            {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            result.push((s, TokenKind::Number));
            continue;
        }

        // 5. Identifiers / Keywords / Types / Functions
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();

            // Lookahead for function call: word followed by '('
            let mut next_idx = i;
            while next_idx < len && chars[next_idx].is_whitespace() && chars[next_idx] != '\n' {
                next_idx += 1;
            }
            let is_fn = next_idx < len && chars[next_idx] == '(';

            if keywords.contains(&word.as_str()) {
                result.push((word, TokenKind::Keyword));
            } else if types.contains(&word.as_str())
                || (word.starts_with(|ch: char| ch.is_uppercase())
                    && (lang == Language::Rust
                        || lang == Language::Go
                        || lang == Language::TypeScript))
            {
                result.push((word, TokenKind::Type));
            } else if is_fn {
                result.push((word, TokenKind::Function));
            } else {
                result.push((word, TokenKind::Variable));
            }
            continue;
        }

        // 6. Operators and Punctuation
        if "{}[]();:,.<>!&|+-*/%^=~?".contains(c) {
            result.push((c.to_string(), TokenKind::Operator));
            i += 1;
            continue;
        }

        // 7. Whitespace and other characters
        result.push((c.to_string(), TokenKind::Text));
        i += 1;
    }

    result
}

fn get_keywords(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        Language::Go => &[
            "break",
            "case",
            "chan",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "fallthrough",
            "for",
            "func",
            "go",
            "goto",
            "if",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
            "true",
            "false",
            "nil",
            "iota",
        ],
        Language::TypeScript | Language::JavaScript => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "finally",
            "for",
            "from",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "return",
            "super",
            "switch",
            "this",
            "throw",
            "try",
            "typeof",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "true",
            "false",
            "null",
            "undefined",
            "interface",
            "type",
            "enum",
            "as",
        ],
        Language::Python => &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
            "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return",
            "True", "try", "while", "with", "yield",
        ],
        Language::C | Language::Cpp => &[
            "auto",
            "break",
            "case",
            "char",
            "const",
            "continue",
            "default",
            "do",
            "double",
            "else",
            "enum",
            "extern",
            "float",
            "for",
            "goto",
            "if",
            "int",
            "long",
            "register",
            "return",
            "short",
            "signed",
            "sizeof",
            "static",
            "struct",
            "switch",
            "typedef",
            "union",
            "unsigned",
            "void",
            "volatile",
            "while",
            "class",
            "public",
            "private",
            "protected",
            "template",
            "typename",
            "new",
            "delete",
            "this",
            "namespace",
            "using",
        ],
        Language::Shell | Language::PowerShell => &[
            "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "until", "do",
            "done", "in", "function", "return", "exit", "local", "export",
        ],
        Language::Json => &["true", "false", "null"],
        Language::Yaml => &["true", "false", "yes", "no", "null"],
        Language::Toml => &["true", "false"],
        _ => &[],
    }
}

fn get_types(lang: Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &[
            "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32",
            "i64", "i128", "isize", "f32", "f64", "String", "Vec", "Option", "Result", "Box",
            "Arc", "Rc", "Path", "PathBuf",
        ],
        Language::Go => &[
            "bool",
            "byte",
            "complex64",
            "complex128",
            "error",
            "float32",
            "float64",
            "int",
            "int8",
            "int16",
            "int32",
            "int64",
            "rune",
            "string",
            "uint",
            "uint8",
            "uint16",
            "uint32",
            "uint64",
            "uintptr",
        ],
        Language::TypeScript => &[
            "string", "number", "boolean", "any", "void", "never", "unknown", "object", "Array",
            "Record", "Promise", "Map", "Set",
        ],
        Language::C | Language::Cpp => &[
            "int", "char", "float", "double", "bool", "void", "size_t", "uint8_t", "uint16_t",
            "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t", "string", "vector",
        ],
        _ => &[],
    }
}

// =========================================================================
// Diff Viewer Implementation (Side-by-Side & Inline)
// Matches Go `tempterm/diff.go:420-590`.
// =========================================================================

/// Kind of line change in a diff.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Equal,
    Added,
    Deleted,
    Modified,
}

/// A line in a diff comparison.
#[derive(Clone, Debug)]
pub struct DiffLine {
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
    pub old_text: String,
    pub new_text: String,
    pub kind: DiffKind,
}

/// Result of computing diff between two texts.
#[derive(Clone, Debug, Default)]
pub struct DiffResult {
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub is_identical: bool,
}

/// Computes line-by-line diff using longest common subsequence with prefix/suffix pruning.
pub fn compute_line_diff(old_text: &str, new_text: &str) -> DiffResult {
    if old_text == new_text {
        let lines: Vec<DiffLine> = old_text
            .lines()
            .enumerate()
            .map(|(idx, line)| DiffLine {
                old_line_num: Some(idx + 1),
                new_line_num: Some(idx + 1),
                old_text: line.to_string(),
                new_text: line.to_string(),
                kind: DiffKind::Equal,
            })
            .collect();
        return DiffResult {
            lines,
            additions: 0,
            deletions: 0,
            is_identical: true,
        };
    }

    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();

    let m = old_lines.len();
    let n = new_lines.len();

    // 1. Strip common prefix lines for O(1) localized edit performance
    let mut prefix_len = 0;
    while prefix_len < m && prefix_len < n && old_lines[prefix_len] == new_lines[prefix_len] {
        prefix_len += 1;
    }

    // 2. Strip common suffix lines
    let mut suffix_len = 0;
    while suffix_len < (m - prefix_len)
        && suffix_len < (n - prefix_len)
        && old_lines[m - 1 - suffix_len] == new_lines[n - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let mut result_lines = Vec::with_capacity(m.max(n));
    let mut additions = 0;
    let mut deletions = 0;

    // Add common prefix lines
    for idx in 0..prefix_len {
        result_lines.push(DiffLine {
            old_line_num: Some(idx + 1),
            new_line_num: Some(idx + 1),
            old_text: old_lines[idx].to_string(),
            new_text: new_lines[idx].to_string(),
            kind: DiffKind::Equal,
        });
    }

    let mid_old = &old_lines[prefix_len..(m - suffix_len)];
    let mid_new = &new_lines[prefix_len..(n - suffix_len)];

    let mid_m = mid_old.len();
    let mid_n = mid_new.len();

    if mid_m > 0 || mid_n > 0 {
        // Guard against massive matrix allocation (>4,000,000 cells)
        if (mid_m as u64) * (mid_n as u64) > 4_000_000 {
            // Fast linear chunk fallback for huge unsynced files
            for (idx, &line) in mid_old.iter().enumerate() {
                result_lines.push(DiffLine {
                    old_line_num: Some(prefix_len + idx + 1),
                    new_line_num: None,
                    old_text: line.to_string(),
                    new_text: String::new(),
                    kind: DiffKind::Deleted,
                });
                deletions += 1;
            }
            for (idx, &line) in mid_new.iter().enumerate() {
                result_lines.push(DiffLine {
                    old_line_num: None,
                    new_line_num: Some(prefix_len + idx + 1),
                    old_text: String::new(),
                    new_text: line.to_string(),
                    kind: DiffKind::Added,
                });
                additions += 1;
            }
        } else {
            // LCS dynamic programming table on middle section
            let mut dp = vec![vec![0usize; mid_n + 1]; mid_m + 1];
            for i in 1..=mid_m {
                for j in 1..=mid_n {
                    if mid_old[i - 1] == mid_new[j - 1] {
                        dp[i][j] = dp[i - 1][j - 1] + 1;
                    } else {
                        dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
                    }
                }
            }

            // Backtrack to build diff
            let mut i = mid_m;
            let mut j = mid_n;
            let mut raw_diff = Vec::new();

            while i > 0 || j > 0 {
                if i > 0 && j > 0 && mid_old[i - 1] == mid_new[j - 1] {
                    raw_diff.push(DiffLine {
                        old_line_num: Some(prefix_len + i),
                        new_line_num: Some(prefix_len + j),
                        old_text: mid_old[i - 1].to_string(),
                        new_text: mid_new[j - 1].to_string(),
                        kind: DiffKind::Equal,
                    });
                    i -= 1;
                    j -= 1;
                } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
                    raw_diff.push(DiffLine {
                        old_line_num: None,
                        new_line_num: Some(prefix_len + j),
                        old_text: String::new(),
                        new_text: mid_new[j - 1].to_string(),
                        kind: DiffKind::Added,
                    });
                    additions += 1;
                    j -= 1;
                } else if i > 0 && (j == 0 || dp[i][j - 1] < dp[i - 1][j]) {
                    raw_diff.push(DiffLine {
                        old_line_num: Some(prefix_len + i),
                        new_line_num: None,
                        old_text: mid_old[i - 1].to_string(),
                        new_text: String::new(),
                        kind: DiffKind::Deleted,
                    });
                    deletions += 1;
                    i -= 1;
                }
            }

            raw_diff.reverse();
            result_lines.extend(raw_diff);
        }
    }

    // Add common suffix lines
    for idx in 0..suffix_len {
        let old_idx = m - suffix_len + idx;
        let new_idx = n - suffix_len + idx;
        result_lines.push(DiffLine {
            old_line_num: Some(old_idx + 1),
            new_line_num: Some(new_idx + 1),
            old_text: old_lines[old_idx].to_string(),
            new_text: new_lines[new_idx].to_string(),
            kind: DiffKind::Equal,
        });
    }

    DiffResult {
        lines: result_lines,
        additions,
        deletions,
        is_identical: additions == 0 && deletions == 0,
    }
}

/// Diff view state holding inputs, diff result, and view toggles.
#[derive(Clone, Debug)]
pub struct DiffViewState {
    pub is_open: bool,
    pub title: String,
    pub scope: String,
    pub old_content: String,
    pub new_content: String,
    pub diff_result: DiffResult,
    pub is_split: bool,
    pub sync_scroll_y: f32,
    pub repo_root: Option<PathBuf>,
    pub rel_path: Option<String>,
    pub is_staged: bool,
    pub action_status: Option<String>,
    pub confirm_discard: bool,
}

impl Default for DiffViewState {
    fn default() -> Self {
        Self {
            is_open: false,
            title: String::new(),
            scope: "Working Tree".to_string(),
            old_content: String::new(),
            new_content: String::new(),
            diff_result: DiffResult::default(),
            is_split: true,
            sync_scroll_y: 0.0,
            repo_root: None,
            rel_path: None,
            is_staged: false,
            action_status: None,
            confirm_discard: false,
        }
    }
}

impl DiffViewState {
    pub fn open(
        &mut self,
        title: &str,
        scope: &str,
        old_content: &str,
        new_content: &str,
        is_split: bool,
    ) {
        self.is_open = true;
        self.title = title.to_string();
        self.scope = scope.to_string();
        self.old_content = old_content.to_string();
        self.new_content = new_content.to_string();
        self.diff_result = compute_line_diff(old_content, new_content);
        self.is_split = is_split;
        self.sync_scroll_y = 0.0;
        self.repo_root = None;
        self.rel_path = None;
        self.is_staged = false;
        self.action_status = None;
        self.confirm_discard = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_with_git(
        &mut self,
        title: &str,
        scope: &str,
        old_content: &str,
        new_content: &str,
        is_split: bool,
        repo_root: Option<PathBuf>,
        rel_path: Option<String>,
        is_staged: bool,
    ) {
        self.open(title, scope, old_content, new_content, is_split);
        self.repo_root = repo_root;
        self.rel_path = rel_path;
        self.is_staged = is_staged;
        self.action_status = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::for_path("src/main.rs"), Language::Rust);
        assert_eq!(Language::for_path("app.go"), Language::Go);
        assert_eq!(Language::for_path("index.ts"), Language::TypeScript);
        assert_eq!(Language::for_path("bundle.js"), Language::JavaScript);
        assert_eq!(Language::for_path("config.json"), Language::Json);
        assert_eq!(Language::for_path("deploy.yml"), Language::Yaml);
        assert_eq!(Language::for_path("Cargo.toml"), Language::Toml);
        assert_eq!(Language::for_path("README.md"), Language::Markdown);
        assert_eq!(Language::for_path("script.py"), Language::Python);
        assert_eq!(Language::for_path("page.html"), Language::HtmlXml);
        assert_eq!(Language::for_path("style.css"), Language::Css);
        assert_eq!(Language::for_path("build.sh"), Language::Shell);
        assert_eq!(Language::for_path("setup.ps1"), Language::PowerShell);
        assert_eq!(Language::for_path("main.c"), Language::C);
        assert_eq!(Language::for_path("core.cpp"), Language::Cpp);
        assert_eq!(Language::for_path("notes.unknown_ext"), Language::PlainText);
    }

    #[test]
    fn test_syntax_tokenization_and_colors() {
        let rust_code = "fn main() {\n    let x: i32 = 42;\n    println!(\"hello\");\n}";
        let tokens = tokenize(rust_code, Language::Rust);
        assert!(tokens
            .iter()
            .any(|(t, k)| t == "fn" && *k == TokenKind::Keyword));
        assert!(tokens
            .iter()
            .any(|(t, k)| t == "main" && *k == TokenKind::Function));
        assert!(tokens
            .iter()
            .any(|(t, k)| t == "i32" && *k == TokenKind::Type));
        assert!(tokens
            .iter()
            .any(|(t, k)| t == "42" && *k == TokenKind::Number));
        assert!(tokens
            .iter()
            .any(|(t, k)| t == "\"hello\"" && *k == TokenKind::String));

        let theme = SyntaxTheme::dark();
        let job = highlight_syntax(rust_code, Language::Rust, &theme, 13.0, 4);
        assert!(!job.text.is_empty());
    }

    #[test]
    fn test_diff_calculation_identical_added_deleted_modified() {
        // 1. Identical text
        let identical = compute_line_diff("line1\nline2\n", "line1\nline2\n");
        assert!(identical.is_identical);
        assert_eq!(identical.additions, 0);
        assert_eq!(identical.deletions, 0);

        // 2. Added and deleted lines
        let old = "apple\nbanana\ncherry\n";
        let new = "apple\nblueberry\ncherry\ndate\n";
        let diff = compute_line_diff(old, new);
        assert!(!diff.is_identical);
        assert_eq!(diff.deletions, 1); // banana
        assert_eq!(diff.additions, 2); // blueberry, date
    }

    #[test]
    fn test_editor_doc_lifecycle_and_atomic_save() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "tt_editor_test_{}",
            temp_terminal_procs::def::sha256_digest(b"editor_test")[0]
        ));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_file = tmp_dir.join("test.rs");
        let initial_text = "fn test() -> bool { true }\n";
        std::fs::write(&test_file, initial_text).unwrap();

        // Open
        let mut doc = EditorDoc::open("test.rs", &tmp_dir).unwrap();
        assert_eq!(doc.content, initial_text);
        assert!(!doc.is_dirty);
        assert_eq!(doc.language, Language::Rust);

        // Edit
        doc.content.push_str("// modified line\n");
        doc.is_dirty = true;
        assert!(doc.is_dirty);

        // Save
        doc.save().unwrap();
        assert!(!doc.is_dirty);

        // Verify on disk
        let saved_text = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(saved_text, doc.content);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_binary_and_size_limits() {
        let binary_bytes = [0x48, 0x65, 0x6c, 0x00, 0x6f];
        assert!(is_binary_data(&binary_bytes));

        let plain_bytes = b"Hello, World!";
        assert!(!is_binary_data(plain_bytes));

        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn test_diff_large_common_prefix_suffix_pruning() {
        // Construct 1000 lines of identical prefix, 2 modified lines, 1000 lines of identical suffix
        let mut old_lines = Vec::new();
        let mut new_lines = Vec::new();

        for i in 1..=1000 {
            old_lines.push(format!("prefix_line_{}", i));
            new_lines.push(format!("prefix_line_{}", i));
        }

        old_lines.push("old_middle_a".to_string());
        old_lines.push("old_middle_b".to_string());

        new_lines.push("new_middle_x".to_string());
        new_lines.push("new_middle_y".to_string());

        for i in 1..=1000 {
            old_lines.push(format!("suffix_line_{}", i));
            new_lines.push(format!("suffix_line_{}", i));
        }

        let old_text = old_lines.join("\n");
        let new_text = new_lines.join("\n");

        let t0 = std::time::Instant::now();
        let res = compute_line_diff(&old_text, &new_text);
        let elapsed = t0.elapsed();

        assert_eq!(res.deletions, 2);
        assert_eq!(res.additions, 2);
        assert_eq!(res.lines.len(), 2004);
        // Pruning should complete very fast (< 100ms)
        assert!(
            elapsed.as_millis() < 500,
            "Diff pruning took too long: {}ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_diff_view_state_git_fields() {
        let mut diff_view = DiffViewState::default();
        assert!(diff_view.repo_root.is_none());
        assert!(!diff_view.is_staged);

        diff_view.open_with_git(
            "main.rs",
            "Working Tree",
            "old",
            "new",
            true,
            Some(PathBuf::from("D:/repo")),
            Some("src/main.rs".to_string()),
            false,
        );

        assert!(diff_view.is_open);
        assert_eq!(diff_view.repo_root, Some(PathBuf::from("D:/repo")));
        assert_eq!(diff_view.rel_path, Some("src/main.rs".to_string()));
        assert!(!diff_view.is_staged);
    }
}
