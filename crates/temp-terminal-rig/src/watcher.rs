use std::sync::LazyLock;
use regex::Regex;
use serde::{Deserialize, Serialize};

static ANSI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b(?:\][^\x07\x1b]*(?:\x07|\x1b\\)|\[[0-?]*[ -/]*[@-~]|[\x20-\x2f]*[\x30-\x7e])")
        .expect("valid ANSI regex")
});

static EXIT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\[process exited|process (?:completed|terminated|exited) with exit code|session closed|session terminated|\[exited with code|exit status \d+)")
        .expect("valid exit regex")
});

static BLOCKED_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\[y/n\]|\[yes/no\]|\(y/n\)|permission required|allow this action|do you want to continue|are you sure|press enter to continue|approve\?|authorize\?|(?:password|passphrase)(?:\s+for\s+[^:]+)?\s*:)")
        .expect("valid blocked regex")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettlementKind {
    Idle,
    Blocked,
    Working,
    Exited,
}

pub struct StreamWatcher;

impl StreamWatcher {
    /// Strips ANSI CSI, OSC, and standard terminal escape sequences.
    pub fn strip_ansi(raw: &str) -> String {
        ANSI_REGEX.replace_all(raw, "").into_owned()
    }

    /// Extracts the bottom `line_count` lines from terminal output,
    /// stripping ANSI sequences and popping trailing empty lines.
    pub fn extract_screen_tail(raw_output: &str, line_count: usize) -> Vec<String> {
        if line_count == 0 || raw_output.is_empty() {
            return Vec::new();
        }

        let cleaned = Self::strip_ansi(raw_output);
        let mut lines: Vec<String> = cleaned
            .lines()
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();

        while let Some(last) = lines.last() {
            if last.trim().is_empty() {
                lines.pop();
            } else {
                break;
            }
        }

        if lines.is_empty() {
            return Vec::new();
        }

        if lines.len() > line_count {
            lines.split_off(lines.len() - line_count)
        } else {
            lines
        }
    }

    /// Detects whether the terminal VT output has settled into an Idle,
    /// Blocked, Working, or Exited state based on harness indicators.
    pub fn detect_settlement(raw_output: &str, harness: &str) -> SettlementKind {
        if raw_output.trim().is_empty() {
            return SettlementKind::Working;
        }

        let tail = Self::extract_screen_tail(raw_output, 12);
        if tail.is_empty() {
            return SettlementKind::Working;
        }

        let tail_text = tail.join("\n");

        if EXIT_REGEX.is_match(&tail_text) {
            return SettlementKind::Exited;
        }

        let bottom_lines: Vec<&str> = tail.iter().rev().take(2).map(|s| s.as_str()).collect();
        let bottom_text = bottom_lines.into_iter().rev().collect::<Vec<&str>>().join("\n");

        if BLOCKED_REGEX.is_match(&bottom_text) {
            return SettlementKind::Blocked;
        }

        let last_line = match tail.last() {
            Some(line) => line,
            None => return SettlementKind::Working,
        };

        let trimmed_last = last_line.trim_end();
        let normalized_harness = harness.to_lowercase();

        let is_idle = match normalized_harness.as_str() {
            "claude" => {
                trimmed_last.ends_with('❯')
                    || trimmed_last.ends_with('$')
                    || trimmed_last.ends_with('#')
            }
            "codex" => {
                trimmed_last.ends_with('›')
                    || trimmed_last.ends_with('$')
                    || trimmed_last.ends_with('#')
            }
            "pi" => {
                trimmed_last.starts_with("pi>")
                    || trimmed_last.starts_with("π>")
                    || trimmed_last.ends_with('>')
                    || trimmed_last.ends_with('$')
            }
            "opencode" => {
                trimmed_last.starts_with("opencode>")
                    || trimmed_last.ends_with('>')
                    || trimmed_last.ends_with('$')
            }
            "omp" => {
                trimmed_last.starts_with("omp>")
                    || trimmed_last.ends_with('❯')
                    || trimmed_last.ends_with('>')
                    || trimmed_last.ends_with('$')
            }
            _ => {
                trimmed_last.ends_with('$')
                    || trimmed_last.ends_with('#')
                    || trimmed_last.ends_with('%')
                    || trimmed_last.ends_with('>')
                    || trimmed_last.ends_with('❯')
                    || trimmed_last.ends_with('›')
            }
        };

        if is_idle {
            SettlementKind::Idle
        } else {
            SettlementKind::Working
        }
    }

    /// Formats an unprompted stop notification when a worker goes idle
    /// without sending an explicit completion message.
    pub fn format_unprompted_stop_notice(
        run_id: u32,
        role: &str,
        screen_tail: &[String],
    ) -> String {
        format!(
            "[watch] {} run #{} is idle without reporting. Screen:\n{}",
            role,
            run_id,
            screen_tail.join("\n")
        )
    }
}
