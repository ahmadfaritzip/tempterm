//! Reading and atomic writing of shared process definitions in `tempterm.yml` or `temp-terminal.yml`.
//! Corresponds to Go `tempterm/internal/procs/file.go`.

use crate::def::Def;
use std::fs;
use std::path::{Path, PathBuf};

/// Primary and legacy file names for shared processes.
pub const FILE_NAME: &str = "tempterm.yml";
pub const ALT_FILE_NAME: &str = "temp-terminal.yml";

/// Resolves the existing or default process file path in the project root.
pub fn resolve_file_path(root: &Path) -> PathBuf {
    let p1 = root.join(FILE_NAME);
    if p1.is_file() {
        return p1;
    }
    let p_oma = root.join("omaterm.yml");
    if p_oma.is_file() {
        return p_oma;
    }
    let p2 = root.join(ALT_FILE_NAME);
    if p2.is_file() {
        return p2;
    }
    p1
}

/// Reads the shared process definitions of the project at `root`.
/// Returns an empty list if no process file exists.
pub fn load_file(root: &Path) -> Result<Vec<Def>, std::io::Error> {
    let path = resolve_file_path(root);
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)?;
    parse_process_yaml(&content)
}

/// Parses a process YAML string into a list of `Def`.
pub fn parse_process_yaml(text: &str) -> Result<Vec<Def>, std::io::Error> {
    let mut defs = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut in_processes = false;
    let mut current_def: Option<Def> = None;
    let mut current_section = ""; // "watch" or "env"

    let finalize_current = |current: &mut Option<Def>, out: &mut Vec<Def>| {
        if let Some(mut d) = current.take() {
            if !d.name.is_empty() {
                d.local = false;
                out.push(d);
            }
        }
    };

    for raw_line in lines {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Top-level key (0 indent)
        if !line.starts_with(' ') && !line.starts_with('\t') {
            if trimmed.starts_with("processes:") {
                in_processes = true;
            } else {
                finalize_current(&mut current_def, &mut defs);
                in_processes = false;
            }
            continue;
        }

        if !in_processes {
            continue;
        }

        // Check indentation level under `processes:`
        let indent = line.len() - line.trim_start().len();

        if indent == 2 {
            // New process definition: `  name:`
            finalize_current(&mut current_def, &mut defs);
            current_section = "";
            let name = trimmed.trim_end_matches(':').trim().to_string();
            let d = Def {
                name,
                ..Default::default()
            };
            current_def = Some(d);
        } else if indent >= 4 {
            if let Some(ref mut d) = current_def {
                if indent == 4 && trimmed.ends_with(':') {
                    // Sub-section like `    watch:` or `    env:`
                    let sec = trimmed.trim_end_matches(':').trim();
                    current_section = match sec {
                        "watch" => "watch",
                        "env" => "env",
                        _ => "",
                    };
                } else if current_section == "watch" && trimmed.starts_with('-') {
                    let glob = trimmed.trim_start_matches('-').trim();
                    let cleaned = glob.trim_matches('\'').trim_matches('"');
                    if !cleaned.is_empty() {
                        d.watch.push(cleaned.to_string());
                    }
                } else if current_section == "env" && trimmed.contains(':') {
                    let (k, v) = trimmed.split_once(':').unwrap_or(("", ""));
                    let k = k.trim().to_string();
                    let v = v.trim().trim_matches('\'').trim_matches('"').to_string();
                    if !k.is_empty() {
                        d.env.insert(k, v);
                    }
                } else if let Some((k, v)) = trimmed.split_once(':') {
                    current_section = "";
                    let k = k.trim();
                    let v = v.trim().trim_matches('\'').trim_matches('"');
                    match k {
                        "command" => d.command = v.to_string(),
                        "dir" => d.dir = v.to_string(),
                        "port" => d.port = v.parse::<i64>().unwrap_or(0),
                        "autostart" => d.autostart = v == "true" || v == "yes",
                        "restart" => d.restart = v.to_string(),
                        "url" => d.url = v.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }

    finalize_current(&mut current_def, &mut defs);
    Ok(defs)
}

/// Saves the given definitions into `tempterm.yml` (or `temp-terminal.yml`) at `root`.
/// Non-process top-level lines and comments are preserved.
pub fn save_file(root: &Path, defs: &[Def]) -> Result<(), std::io::Error> {
    let path = resolve_file_path(root);
    let mut non_process_lines = Vec::new();

    if path.is_file() {
        if let Ok(content) = fs::read_to_string(&path) {
            let mut skipping_processes = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if !line.starts_with(' ')
                    && !line.starts_with('\t')
                    && trimmed.starts_with("processes:")
                {
                    skipping_processes = true;
                    continue;
                }
                if skipping_processes {
                    if !line.starts_with(' ')
                        && !line.starts_with('\t')
                        && !trimmed.is_empty()
                        && !trimmed.starts_with('#')
                    {
                        skipping_processes = false;
                        non_process_lines.push(line.to_string());
                    }
                } else {
                    non_process_lines.push(line.to_string());
                }
            }
        }
    }

    let mut out = String::new();
    // Preserve existing header lines before processes
    for line in &non_process_lines {
        out.push_str(line);
        out.push('\n');
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }

    out.push_str("processes:\n");
    for d in defs {
        if d.local {
            continue;
        }
        out.push_str(&format!("  {}:\n", d.name));
        out.push_str(&format!("    command: {}\n", d.command));
        if !d.dir.is_empty() && d.dir != "." {
            out.push_str(&format!("    dir: {}\n", d.dir));
        }
        if d.port > 0 {
            out.push_str(&format!("    port: {}\n", d.port));
        }
        if d.autostart {
            out.push_str("    autostart: true\n");
        }
        if !d.restart.is_empty() && d.restart != "never" {
            out.push_str(&format!("    restart: {}\n", d.restart));
        }
        if !d.url.is_empty() {
            out.push_str(&format!("    url: {}\n", d.url));
        }
        if !d.watch.is_empty() {
            out.push_str("    watch:\n");
            for w in &d.watch {
                out.push_str(&format!("      - {}\n", w));
            }
        }
        if !d.env.is_empty() {
            out.push_str("    env:\n");
            for (k, v) in &d.env {
                out.push_str(&format!("      {}: \"{}\"\n", k, v));
            }
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, out.as_bytes())?;
    fs::rename(&tmp_path, &path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_roundtrip_and_preservation() {
        let yaml_sample = r#"# Shared configuration
name: sample_project
version: 1

processes:
  web:
    command: npm run dev
    dir: frontend
    port: 3000
    autostart: true
    restart: always
    watch:
      - src/**
      - public/**
    env:
      NODE_ENV: "development"
      API_URL: "http://localhost:8080"
  worker:
    command: cargo run --bin worker
    restart: on-failure
"#;

        let defs = parse_process_yaml(yaml_sample).expect("parse should succeed");
        assert_eq!(defs.len(), 2);

        let web = &defs[0];
        assert_eq!(web.name, "web");
        assert_eq!(web.command, "npm run dev");
        assert_eq!(web.dir, "frontend");
        assert_eq!(web.port, 3000);
        assert!(web.autostart);
        assert_eq!(web.restart, "always");
        assert_eq!(web.watch, vec!["src/**", "public/**"]);
        assert_eq!(
            web.env.get("NODE_ENV").map(|s| s.as_str()),
            Some("development")
        );
        assert_eq!(
            web.env.get("API_URL").map(|s| s.as_str()),
            Some("http://localhost:8080")
        );

        let worker = &defs[1];
        assert_eq!(worker.name, "worker");
        assert_eq!(worker.command, "cargo run --bin worker");
        assert_eq!(worker.restart, "on-failure");
        assert_eq!(worker.port, 0);

        // Test saving back
        let dir = std::env::temp_dir().join(format!("tt_yaml_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let orig_file = dir.join(FILE_NAME);
        fs::write(&orig_file, yaml_sample).unwrap();

        // Add a third process and save
        let mut defs_mut = defs;
        let api = Def {
            name: "api".to_string(),
            command: "go run .".to_string(),
            port: 8080,
            ..Default::default()
        };
        defs_mut.push(api);

        save_file(&dir, &defs_mut).expect("save should succeed");

        let reloaded = load_file(&dir).expect("reloaded should succeed");
        assert_eq!(reloaded.len(), 3);
        assert_eq!(reloaded[2].name, "api");
        assert_eq!(reloaded[2].port, 8080);

        // Verify top-level comment and name: sample_project preserved
        let reloaded_raw = fs::read_to_string(&orig_file).unwrap();
        assert!(reloaded_raw.contains("# Shared configuration"));
        assert!(reloaded_raw.contains("name: sample_project"));

        let _ = fs::remove_dir_all(&dir);
    }
}
