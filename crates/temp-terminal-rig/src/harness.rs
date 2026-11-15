use crate::error::RigError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessSpec {
    pub kind: Option<&'static str>,
    pub launch: bool,
    pub model_flags: &'static [&'static str],
    pub effort_flags: &'static [&'static str],
    pub interactive_args: &'static [&'static str],
    pub headless_args: &'static [&'static str],
    pub resume_args: &'static [&'static str],
}

pub fn get_harness_spec(harness: &str) -> Option<HarnessSpec> {
    match harness.trim().to_ascii_lowercase().as_str() {
        "claude" => Some(HarnessSpec {
            kind: Some("claude"),
            launch: true,
            model_flags: &["--model"],
            effort_flags: &["--effort"],
            interactive_args: &["claude", "--dangerously-skip-permissions"],
            headless_args: &["claude", "-p", "--dangerously-skip-permissions", "--session-id"],
            resume_args: &["claude", "--resume"],
        }),
        "codex" => Some(HarnessSpec {
            kind: Some("codex"),
            launch: true,
            model_flags: &["-m"],
            effort_flags: &["-c", "model_reasoning_effort="],
            interactive_args: &["codex", "--dangerously-bypass-approvals-and-sandbox"],
            headless_args: &["codex", "exec", "--dangerously-bypass-approvals-and-sandbox"],
            resume_args: &["codex", "resume"],
        }),
        "opencode" => Some(HarnessSpec {
            kind: Some("opencode"),
            launch: true,
            model_flags: &["-m"],
            effort_flags: &[],
            interactive_args: &["opencode", "--auto", "-s", "--prompt"],
            headless_args: &["opencode", "run", "--auto", "-s"],
            resume_args: &["opencode", "-s"],
        }),
        "pi" => Some(HarnessSpec {
            kind: Some("pi"),
            launch: true,
            model_flags: &["--model"],
            effort_flags: &["--thinking"],
            interactive_args: &["pi", "--session"],
            headless_args: &["pi", "-p", "--session-id"],
            resume_args: &["pi", "--resume"],
        }),
        "omp" => Some(HarnessSpec {
            kind: Some("omp"),
            launch: true,
            model_flags: &["-m"],
            effort_flags: &[],
            interactive_args: &["omp", "--auto"],
            headless_args: &["omp", "-p"],
            resume_args: &["omp", "--resume"],
        }),
        "commandcode" => Some(HarnessSpec {
            kind: Some("commandcode"),
            launch: true,
            model_flags: &["--model"],
            effort_flags: &["--effort"],
            interactive_args: &["commandcode", "--dangerously-skip-permissions"],
            headless_args: &["commandcode", "-p", "--dangerously-skip-permissions", "--session-id"],
            resume_args: &["commandcode", "--resume"],
        }),
        _ => None,
    }
}

pub fn expand_argv(
    harness: &str,
    launch_kind: &str,
    model: Option<&str>,
    effort: Option<&str>,
    sid: &str,
    prompt: Option<&str>,
) -> Result<Vec<String>, RigError> {
    let harness_clean = harness.trim();
    let harness_lower = harness_clean.to_ascii_lowercase();
    let kind_lower = launch_kind.trim().to_ascii_lowercase();

    let valid_harnesses = ["claude", "codex", "opencode", "pi", "omp", "commandcode"];
    if !valid_harnesses.contains(&harness_lower.as_str()) {
        return Err(RigError::InvalidParameter(format!(
            "unknown harness: {harness_clean}"
        )));
    }

    let mut args: Vec<String> = Vec::new();

    match kind_lower.as_str() {
        "interactive" => match harness_lower.as_str() {
            "claude" => {
                args.push("claude".to_string());
                args.push("--dangerously-skip-permissions".to_string());
                if let Some(m) = model {
                    args.push("--model".to_string());
                    args.push(m.to_string());
                }
                if let Some(e) = effort {
                    args.push("--effort".to_string());
                    args.push(e.to_string());
                }
            }
            "codex" => {
                args.push("codex".to_string());
                args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                if let Some(m) = model {
                    args.push("-m".to_string());
                    args.push(m.to_string());
                }
                if let Some(e) = effort {
                    args.push("-c".to_string());
                    args.push(format!("model_reasoning_effort={e}"));
                }
            }
            "opencode" => {
                args.push("opencode".to_string());
                args.push("--auto".to_string());
                args.push("-s".to_string());
                args.push(format!("ses_{sid}"));
                args.push("--prompt".to_string());
            }
            "pi" => {
                args.push("pi".to_string());
                args.push("--session".to_string());
                args.push(sid.to_string());
                if let Some(m) = model {
                    args.push("--model".to_string());
                    args.push(m.to_string());
                }
                if let Some(e) = effort {
                    args.push("--thinking".to_string());
                    args.push(e.to_string());
                }
            }
            "omp" => {
                args.push("omp".to_string());
                args.push("--auto".to_string());
                if let Some(m) = model {
                    args.push("-m".to_string());
                    args.push(m.to_string());
                }
            }
            "commandcode" => {
                args.push("commandcode".to_string());
                args.push("--dangerously-skip-permissions".to_string());
                if let Some(m) = model {
                    args.push("--model".to_string());
                    args.push(m.to_string());
                }
                if let Some(e) = effort {
                    args.push("--effort".to_string());
                    args.push(e.to_string());
                }
            }
            _ => unreachable!(),
        },
        "headless" => {
            match harness_lower.as_str() {
                "claude" => {
                    args.push("claude".to_string());
                    args.push("-p".to_string());
                    args.push("--dangerously-skip-permissions".to_string());
                    args.push("--session-id".to_string());
                    args.push(sid.to_string());
                    if let Some(m) = model {
                        args.push("--model".to_string());
                        args.push(m.to_string());
                    }
                    if let Some(e) = effort {
                        args.push("--effort".to_string());
                        args.push(e.to_string());
                    }
                }
                "codex" => {
                    args.push("codex".to_string());
                    args.push("exec".to_string());
                    args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                    if let Some(m) = model {
                        args.push("-m".to_string());
                        args.push(m.to_string());
                    }
                    if let Some(e) = effort {
                        args.push("-c".to_string());
                        args.push(format!("model_reasoning_effort={e}"));
                    }
                }
                "opencode" => {
                    args.push("opencode".to_string());
                    args.push("run".to_string());
                    args.push("--auto".to_string());
                    args.push("-s".to_string());
                    args.push(format!("ses_{sid}"));
                    if let Some(m) = model {
                        args.push("-m".to_string());
                        if let Some(e) = effort {
                            if !e.is_empty() {
                                args.push(format!("{m}#{e}"));
                            } else {
                                args.push(m.to_string());
                            }
                        } else {
                            args.push(m.to_string());
                        }
                    }
                }
                "pi" => {
                    args.push("pi".to_string());
                    args.push("-p".to_string());
                    args.push("--session-id".to_string());
                    args.push(sid.to_string());
                    if let Some(m) = model {
                        args.push("--model".to_string());
                        args.push(m.to_string());
                    }
                    if let Some(e) = effort {
                        args.push("--thinking".to_string());
                        args.push(e.to_string());
                    }
                }
                "omp" => {
                    args.push("omp".to_string());
                    args.push("-p".to_string());
                    if let Some(m) = model {
                        args.push("-m".to_string());
                        args.push(m.to_string());
                    }
                }
                "commandcode" => {
                    args.push("commandcode".to_string());
                    args.push("-p".to_string());
                    args.push("--dangerously-skip-permissions".to_string());
                    args.push("--session-id".to_string());
                    args.push(sid.to_string());
                    if let Some(m) = model {
                        args.push("--model".to_string());
                        args.push(m.to_string());
                    }
                    if let Some(e) = effort {
                        args.push("--effort".to_string());
                        args.push(e.to_string());
                    }
                }
                _ => unreachable!(),
            }
            if let Some(p) = prompt {
                args.push(p.to_string());
            }
        }
        "resume" => match harness_lower.as_str() {
            "claude" => {
                args.push("claude".to_string());
                args.push("--resume".to_string());
                args.push(sid.to_string());
            }
            "codex" => {
                args.push("codex".to_string());
                args.push("resume".to_string());
                args.push(sid.to_string());
            }
            "opencode" => {
                args.push("opencode".to_string());
                args.push("-s".to_string());
                args.push(format!("ses_{sid}"));
            }
            "pi" => {
                args.push("pi".to_string());
                args.push("--resume".to_string());
                args.push(sid.to_string());
            }
            "omp" => {
                args.push("omp".to_string());
                args.push("--resume".to_string());
                args.push(sid.to_string());
            }
            "commandcode" => {
                args.push("commandcode".to_string());
                args.push("--resume".to_string());
                args.push(sid.to_string());
            }
            _ => unreachable!(),
        },
        other => {
            return Err(RigError::InvalidParameter(format!(
                "unknown launch kind: {other}"
            )));
        }
    }

    Ok(args)
}
