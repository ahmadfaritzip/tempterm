//! CLI argument parsing, flags, and subcommand dispatcher.
//! Corresponds to Go `tempterm/internal/cli/cli.go`, `process.go`, and `mail.go`.

use std::collections::HashMap;
use std::time::Duration;

use temp_terminal_proto::proto::{
    Mail, MailAskParams, MailAwaitParams, MailListParams, MailReplyParams, Process, ProcessParams,
};

use crate::client::{Client, ClientError};

#[allow(dead_code)]
pub const VERSION: &str = "0.1.0";
pub const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Flags {
    pub vals: HashMap<String, String>,
    pub set: HashMap<String, bool>,
    pub pos: Vec<String>,
    pub rest: Vec<String>,
}

pub fn parse_flags(
    args: &[String],
    valued: &HashMap<&str, &str>,
    boolean: &HashMap<&str, &str>,
) -> Result<Flags, String> {
    let mut f = Flags::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            f.rest = args[i + 1..].to_vec();
            break;
        }
        if !a.starts_with('-') || a == "-" {
            f.pos.push(a.clone());
            i += 1;
            continue;
        }

        let trimmed = a.trim_start_matches('-');
        let (name, val_opt) = if let Some(idx) = trimmed.find('=') {
            (&trimmed[..idx], Some(trimmed[idx + 1..].to_string()))
        } else {
            (trimmed, None)
        };

        if let Some(canon) = valued.get(name) {
            let val = match val_opt {
                Some(v) => v,
                None => {
                    if i + 1 >= args.len() {
                        return Err(format!("{} needs a value", a));
                    }
                    i += 1;
                    args[i].clone()
                }
            };
            f.vals.insert(canon.to_string(), val);
        } else if let Some(canon) = boolean.get(name) {
            f.set.insert(canon.to_string(), true);
        } else {
            return Err(format!("unknown flag {}", a));
        }
        i += 1;
    }
    Ok(f)
}

pub fn parse_timeout(s: &str) -> Result<u64, String> {
    if s.is_empty() {
        return Ok(0);
    }
    let s = s.trim();
    if let Some(sec_str) = s.strip_suffix('s') {
        if let Ok(n) = sec_str.parse::<u64>() {
            return Ok(n * 1000);
        }
    }
    if let Some(ms_str) = s.strip_suffix("ms") {
        if let Ok(n) = ms_str.parse::<u64>() {
            return Ok(n);
        }
    }
    if let Some(m_str) = s.strip_suffix('m') {
        if let Ok(n) = m_str.parse::<u64>() {
            return Ok(n * 60 * 1000);
        }
    }
    if let Ok(n) = s.parse::<u64>() {
        return Ok(n * 1000);
    }
    Err(format!("invalid duration: {}", s))
}

pub fn resolve_project(flag_project: Option<&str>) -> String {
    if let Some(p) = flag_project {
        if !p.is_empty() {
            return p.to_string();
        }
    }
    if let Ok(p) = std::env::var("TEMPTERM_PROJECT") {
        if !p.is_empty() {
            return p;
        }
    }
    if let Ok(p) = std::env::var("TEMP_TERMINAL_PROJECT") {
        if !p.is_empty() {
            return p;
        }
    }
    if let Ok(p) = std::env::var("OMATERM_PROJECT") {
        if !p.is_empty() {
            return p;
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = cwd.file_name() {
            return name.to_string_lossy().to_string();
        }
    }
    "default".to_string()
}

pub fn resolve_pane() -> String {
    std::env::var("TEMPTERM_PANE")
        .or_else(|_| std::env::var("TEMP_TERMINAL_PANE"))
        .or_else(|_| std::env::var("OMATERM_PANE"))
        .unwrap_or_default()
}

// -----------------------------------------------------------------------------
// Command Handlers
// -----------------------------------------------------------------------------

pub async fn run_ps(client: &mut Client, args: &[String]) -> Result<(), ClientError> {
    let mut valued = HashMap::new();
    valued.insert("project", "project");
    valued.insert("p", "project");
    let mut boolean = HashMap::new();
    boolean.insert("json", "json");

    let f = parse_flags(args, &valued, &boolean).map_err(ClientError::Usage)?;
    if !f.pos.is_empty() {
        return Err(ClientError::Usage("temp-terminal ps [--json]".to_string()));
    }

    let project = resolve_project(f.vals.get("project").map(|s| s.as_str()));
    let params = serde_json::to_value(ProcessParams {
        project,
        name: String::new(),
        via: String::new(),
    })?;

    let res = client.call("process.list", params).await?;
    let list: Vec<Process> = serde_json::from_value(res)?;

    if f.set.get("json").copied().unwrap_or(false) {
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }

    if list.is_empty() {
        eprintln!("no processes defined: add one in temp-terminal");
        return Ok(());
    }

    println!(
        "{:<16} {:<10} {:<6} {:<7} {:<9} {:<28} MESSAGE",
        "NAME", "STATUS", "PORT", "PID", "RESTARTS", "URL"
    );
    for p in list {
        let port_str = if p.port > 0 {
            p.port.to_string()
        } else {
            "-".to_string()
        };
        let pid_str = if p.pid > 0 {
            p.pid.to_string()
        } else {
            "-".to_string()
        };
        let url_str = if !p.url.is_empty() {
            p.url.clone()
        } else {
            "-".to_string()
        };
        let msg_str = if !p.message.is_empty() {
            p.message.clone()
        } else {
            "-".to_string()
        };

        println!(
            "{:<16} {:<10} {:<6} {:<7} {:<9} {:<28} {}",
            p.name, p.status, port_str, pid_str, p.restarts, url_str, msg_str
        );
    }

    Ok(())
}

pub async fn run_process_cmd(
    client: &mut Client,
    cmd: &str,
    args: &[String],
) -> Result<(), ClientError> {
    let mut valued = HashMap::new();
    valued.insert("timeout", "timeout");
    valued.insert("t", "timeout");
    valued.insert("project", "project");
    valued.insert("p", "project");
    let mut boolean = HashMap::new();
    boolean.insert("wait", "wait");

    let f = parse_flags(args, &valued, &boolean).map_err(ClientError::Usage)?;
    if f.pos.len() != 1 {
        let usage = match cmd {
            "stop" => "temp-terminal stop <name> [-p project]",
            "start" => "temp-terminal start <name> [--wait] [-t 60s] [-p project]",
            "restart" => "temp-terminal restart <name> [--wait] [-t 60s] [-p project]",
            "trust" => "temp-terminal trust <name> [-p project]",
            _ => "temp-terminal <command> <name>",
        };
        return Err(ClientError::Usage(usage.to_string()));
    }

    let name = &f.pos[0];
    let project = resolve_project(f.vals.get("project").map(|s| s.as_str()));
    let method = match cmd {
        "start" => "process.start",
        "stop" => "process.stop",
        "restart" => "process.restart",
        "trust" => "process.trust",
        other => other,
    };

    let params = serde_json::to_value(ProcessParams {
        project: project.clone(),
        name: name.clone(),
        via: String::new(),
    })?;

    let res = client.call(method, params).await?;
    let p: Process = serde_json::from_value(res)?;

    let wait = f.set.get("wait").copied().unwrap_or(false);
    if !wait || cmd == "stop" || cmd == "trust" {
        println!("{} is {}", name, p.status);
        return Ok(());
    }

    // Wait logic
    let timeout_ms = match f.vals.get("timeout") {
        Some(t) => parse_timeout(t).map_err(ClientError::Usage)?,
        None => DEFAULT_PROCESS_TIMEOUT.as_millis() as u64,
    };

    let start = std::time::Instant::now();
    let duration = Duration::from_millis(timeout_ms);
    loop {
        let list_res = client
            .call(
                "process.list",
                serde_json::to_value(ProcessParams {
                    project: project.clone(),
                    name: String::new(),
                    via: String::new(),
                })?,
            )
            .await?;
        let list: Vec<Process> = serde_json::from_value(list_res)?;
        if let Some(proc) = list.iter().find(|i| &i.name == name) {
            if proc.status == "running" || (proc.status == "exited" && proc.last_exit == 0) {
                println!("{} is {}", name, proc.status);
                return Ok(());
            }
            if proc.status == "failed"
                || proc.status == "stopped"
                || proc.status == "untrusted"
                || (proc.status == "exited" && proc.last_exit != 0)
            {
                let msg = if !proc.message.is_empty() {
                    &proc.message
                } else {
                    &proc.status
                };
                return Err(ClientError::Other(format!("{} is {}", name, msg)));
            }
        }
        if start.elapsed() >= duration {
            return Err(ClientError::Timeout(format!(
                "{} didn't come up in {:?}",
                name, duration
            )));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

pub async fn run_ask(client: &mut Client, args: &[String]) -> Result<(), ClientError> {
    let mut valued = HashMap::new();
    valued.insert("timeout", "timeout");
    valued.insert("t", "timeout");
    valued.insert("file", "file");
    let mut boolean = HashMap::new();
    boolean.insert("no-wait", "no-wait");
    boolean.insert("json", "json");

    let f = parse_flags(args, &valued, &boolean).map_err(ClientError::Usage)?;
    if f.pos.is_empty() || (f.pos.len() < 2 && !f.vals.contains_key("file")) {
        return Err(ClientError::Usage(
            "temp-terminal ask <target> <text…> [--no-wait] [-t timeout]".to_string(),
        ));
    }

    let target = &f.pos[0];
    let text = if let Some(file_path) = f.vals.get("file") {
        if f.pos.len() > 1 {
            return Err(ClientError::Usage(
                "the text, or --file, not both".to_string(),
            ));
        }
        std::fs::read_to_string(file_path).map_err(|e| ClientError::Other(e.to_string()))?
    } else {
        f.pos[1..].join(" ")
    };

    let from_pane = resolve_pane();
    let no_wait = f.set.get("no-wait").copied().unwrap_or(false);
    let is_json = f.set.get("json").copied().unwrap_or(false);

    let ask_params = MailAskParams {
        from: from_pane,
        from_name: String::new(),
        to: target.clone(),
        to_name: String::new(),
        text,
        note: false,
        no_wait,
    };

    let res = client
        .call("mail.ask", serde_json::to_value(ask_params)?)
        .await?;
    let mail: Mail = serde_json::from_value(res)?;

    if no_wait {
        if is_json {
            println!("{}", serde_json::to_string_pretty(&mail)?);
        } else {
            println!("{}", mail.id);
        }
        return Ok(());
    }

    let timeout_ms = match f.vals.get("timeout") {
        Some(t) => parse_timeout(t).map_err(ClientError::Usage)? as i64,
        None => 0,
    };

    let await_params = MailAwaitParams {
        id: mail.id,
        timeout_ms,
    };

    let await_res = client
        .call("mail.await", serde_json::to_value(await_params)?)
        .await?;
    let answered: Mail = serde_json::from_value(await_res)?;

    if is_json {
        println!("{}", serde_json::to_string_pretty(&answered)?);
        return Ok(());
    }

    if answered.state == "replied" {
        println!("{}", answered.reply.trim_end());
    } else if answered.state == "noreply" {
        eprintln!("stopped without answering: {}.", answered.why);
    } else {
        return Err(ClientError::Other(format!(
            "{} wasn't delivered: {}",
            answered.id, answered.why
        )));
    }

    Ok(())
}

pub async fn run_reply(client: &mut Client, args: &[String]) -> Result<(), ClientError> {
    let mut valued = HashMap::new();
    valued.insert("file", "file");
    let boolean = HashMap::new();

    let f = parse_flags(args, &valued, &boolean).map_err(ClientError::Usage)?;
    let mut words = f.pos.clone();
    let mut id = String::new();

    if !words.is_empty()
        && words[0].starts_with('m')
        && words[0][1..].chars().all(|c| c.is_ascii_digit())
    {
        id = words.remove(0);
    }

    let text = if let Some(file_path) = f.vals.get("file") {
        if !words.is_empty() {
            return Err(ClientError::Usage(
                "the text, or --file, not both".to_string(),
            ));
        }
        std::fs::read_to_string(file_path).map_err(|e| ClientError::Other(e.to_string()))?
    } else {
        if words.is_empty() {
            return Err(ClientError::Usage(
                "temp-terminal reply [id] <text…> [--file path]".to_string(),
            ));
        }
        words.join(" ")
    };

    let from_pane = resolve_pane();
    let reply_params = MailReplyParams {
        id,
        from: from_pane,
        text,
    };

    let res = client
        .call("mail.reply", serde_json::to_value(reply_params)?)
        .await?;
    let m: Mail = serde_json::from_value(res)?;

    if m.from_name.is_empty() {
        println!("answered {}", m.id);
    } else {
        println!("answered {} from {}", m.id, m.from_name);
    }

    Ok(())
}

pub async fn run_inbox(client: &mut Client, args: &[String]) -> Result<(), ClientError> {
    let valued = HashMap::new();
    let mut boolean = HashMap::new();
    boolean.insert("json", "json");

    let f = parse_flags(args, &valued, &boolean).map_err(ClientError::Usage)?;
    let pane = resolve_pane();
    let is_json = f.set.get("json").copied().unwrap_or(false);

    let res = client
        .call("mail.list", serde_json::to_value(MailListParams { pane })?)
        .await?;
    let list: Vec<Mail> = serde_json::from_value(res)?;

    if is_json {
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }

    println!(
        "{:<5} {:<14} {:<14} {:<9} TEXT",
        "ID", "FROM", "TO", "STATE"
    );
    for m in list {
        let from = if !m.from_name.is_empty() {
            m.from_name
        } else if m.from.is_empty() {
            "temp-terminal".to_string()
        } else {
            m.from
        };
        let to = if !m.to_name.is_empty() {
            m.to_name
        } else {
            m.to
        };
        let text = m.text.lines().next().unwrap_or("").trim();
        println!(
            "{:<5} {:<14} {:<14} {:<9} {}",
            m.id, from, to, m.state, text
        );
    }

    Ok(())
}

pub async fn run_daemon_cmd(client: &mut Client, subcmd: &str) -> Result<(), ClientError> {
    match subcmd {
        "ping" => {
            let res = client.call("daemon.ping", serde_json::Value::Null).await?;
            let status = res["status"].as_str().unwrap_or("ok");
            println!("daemon: {}", status);
            Ok(())
        }
        "shutdown" => {
            let _ = client
                .call("daemon.shutdown", serde_json::Value::Null)
                .await?;
            println!("daemon shutdown initiated");
            Ok(())
        }
        _ => Err(ClientError::Usage(format!(
            "unknown daemon action {}",
            subcmd
        ))),
    }
}
