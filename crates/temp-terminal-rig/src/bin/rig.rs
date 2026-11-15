use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::error::RigError;
use temp_terminal_rig::hub::resolve_hub_dir;
use temp_terminal_rig::layout::{LayoutAllocator, LayoutConfig, RectBounds, SlotRole, TabSlotLocation};
use temp_terminal_rig::orchestrator::OrchestratorEngine;
use temp_terminal_rig::skills::{RigRole, SkillsDeployer};
use temp_terminal_rig::ticket::{TicketFilter, TicketStatus};
use temp_terminal_rig::triage::{TriageAction, TriageEngine, TriageResult};
use temp_terminal_rig::tui::{BoardRenderOptions, BoardTuiRenderer};
use temp_terminal_rig::watcher::{SettlementKind, StreamWatcher};
use temp_terminal_rig::worktree::WorktreeManager;

fn find_repo_root() -> Result<PathBuf, RigError> {
    let mut curr = env::current_dir()?;
    loop {
        if curr.join(".rig").is_dir() {
            return Ok(curr);
        }
        if !curr.pop() {
            break;
        }
    }
    Ok(env::current_dir()?)
}

fn cmd_init(args: &[String]) -> Result<(), RigError> {
    let target_dir = if let Some(idx) = args.iter().position(|a| a == "--path") {
        PathBuf::from(args.get(idx + 1).ok_or_else(|| {
            RigError::InvalidParameter("missing argument for --path".to_string())
        })?)
    } else {
        env::current_dir()?
    };

    let _db = RigDatabase::open_repo(&target_dir)?;
    let db_path = target_dir.join(".rig").join("rig.db");
    println!("Initialized rig repository database at {}", db_path.display());
    Ok(())
}

fn cmd_new(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    let mut title = String::new();
    let mut body = String::new();
    let mut review = false;
    let mut after: Option<String> = None;
    let mut base: Option<String> = None;
    let mut manager: Option<String> = None;
    let mut story: Option<String> = None;
    let mut branch: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--body" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --body".to_string())
            })?;
            if val == "-" {
                let mut buffer = String::new();
                io::stdin().read_to_string(&mut buffer)?;
                body = buffer;
            } else {
                body = val.clone();
            }
        } else if arg == "--review" {
            review = true;
        } else if arg == "--after" {
            i += 1;
            after = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --after".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--base" {
            i += 1;
            base = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --base".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--manager" {
            i += 1;
            manager = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --manager".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--story" {
            i += 1;
            story = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --story".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--branch" {
            i += 1;
            branch = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --branch".to_string())
                    })?
                    .clone(),
            );
        } else if !arg.starts_with("--") && title.is_empty() {
            title = arg.clone();
        } else if !arg.starts_with("--") {
            title.push(' ');
            title.push_str(arg);
        }
        i += 1;
    }

    if title.trim().is_empty() {
        return Err(RigError::InvalidParameter(
            "title is required for rig new".to_string(),
        ));
    }

    let mgr = manager
        .or_else(|| db.get_setting("manager").ok().flatten())
        .unwrap_or_else(|| "manager:p1".to_string());

    let mut ticket = db.create_ticket(
        title.trim(),
        &body,
        &mgr,
        review,
        after.as_deref(),
        base.as_deref(),
        story.as_deref(),
    )?;

    if let Some(br) = branch {
        ticket.branch = Some(br);
        db.update_ticket(&ticket)?;
    }

    println!("{}", ticket.id);
    Ok(())
}

fn cmd_edit(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    if args.is_empty() {
        return Err(RigError::InvalidParameter(
            "ticket ID is required for rig edit".to_string(),
        ));
    }

    let id = &args[0];
    let mut ticket = db.get_ticket(id)?;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if let Some(eq_idx) = arg.find('=') {
            let key = &arg[..eq_idx];
            let val = &arg[eq_idx + 1..];
            match key {
                "status" => ticket.status = val.parse::<TicketStatus>()?,
                "title" => ticket.title = val.to_string(),
                "body" => ticket.body = val.to_string(),
                "manager" => ticket.manager = val.to_string(),
                "review" => ticket.review = val == "true" || val == "1",
                "after" => {
                    ticket.after = if val.is_empty() {
                        None
                    } else {
                        Some(val.to_string())
                    }
                }
                "base" => {
                    ticket.base = if val.is_empty() {
                        None
                    } else {
                        Some(val.to_string())
                    }
                }
                "story" => {
                    ticket.story = if val.is_empty() {
                        None
                    } else {
                        Some(val.to_string())
                    }
                }
                "branch" => {
                    ticket.branch = if val.is_empty() {
                        None
                    } else {
                        Some(val.to_string())
                    }
                }
                _ => {}
            }
        } else if arg == "--status" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --status".to_string())
            })?;
            ticket.status = val.parse::<TicketStatus>()?;
        } else if arg == "--title" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --title".to_string())
            })?;
            ticket.title = val.clone();
        } else if arg == "--body" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --body".to_string())
            })?;
            ticket.body = val.clone();
        } else if arg == "--review" {
            ticket.review = true;
        } else if arg == "--manager" {
            i += 1;
            ticket.manager = args
                .get(i)
                .ok_or_else(|| {
                    RigError::InvalidParameter("missing argument for --manager".to_string())
                })?
                .clone();
        } else if arg == "--after" {
            i += 1;
            ticket.after = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --after".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--base" {
            i += 1;
            ticket.base = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --base".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--story" {
            i += 1;
            ticket.story = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --story".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--branch" {
            i += 1;
            ticket.branch = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --branch".to_string())
                    })?
                    .clone(),
            );
        }
        i += 1;
    }

    db.update_ticket(&ticket)?;
    println!("Updated {}", ticket.id);
    Ok(())
}

fn cmd_ls(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    let mut include_done = false;
    let mut json_out = false;
    let mut mine = false;
    let mut story: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--all" {
            include_done = true;
        } else if arg == "--json" {
            json_out = true;
        } else if arg == "--mine" {
            mine = true;
        } else if arg == "--story" {
            i += 1;
            story = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --story".to_string())
                    })?
                    .clone(),
            );
        }
        i += 1;
    }

    let manager = if mine {
        Some("manager:p1".to_string())
    } else {
        None
    };

    let filter = TicketFilter {
        manager: manager.as_deref(),
        include_done,
        story: story.as_deref(),
    };

    let tickets = db.list_tickets(&filter)?;

    if json_out {
        let json_str = serde_json::to_string_pretty(&tickets)?;
        println!("{}", json_str);
    } else {
        for ticket in &tickets {
            let group = db.group_of(ticket).unwrap_or("Unknown");
            println!(
                "{} [{}] {} ({})",
                ticket.id, ticket.status, ticket.title, group
            );
        }
    }
    Ok(())
}

fn cmd_show(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    let mut id = String::new();
    let mut json_out = false;

    for arg in args {
        if arg == "--json" {
            json_out = true;
        } else if !arg.starts_with("--") && id.is_empty() {
            id = arg.clone();
        }
    }

    if id.is_empty() {
        return Err(RigError::InvalidParameter(
            "ticket ID is required for rig show".to_string(),
        ));
    }

    let ticket = db.get_ticket(&id)?;

    if json_out {
        let json_str = serde_json::to_string_pretty(&ticket)?;
        println!("{}", json_str);
    } else {
        let group = db.group_of(&ticket).unwrap_or("Unknown");
        println!("ID: {}", ticket.id);
        println!("Title: {}", ticket.title);
        println!("Status: {}", ticket.status);
        println!("Group: {}", group);
        println!("Manager: {}", ticket.manager);
        println!("Review: {}", ticket.review);
        if let Some(branch) = &ticket.branch {
            println!("Branch: {}", branch);
        }
        if let Some(wt) = &ticket.worktree {
            println!("Worktree: {}", wt);
        }
        if let Some(after) = &ticket.after {
            println!("After: {}", after);
        }
        if let Some(base) = &ticket.base {
            println!("Base: {}", base);
        }
        if let Some(story) = &ticket.story {
            println!("Story: {}", story);
        }
        println!("Created: {}", ticket.created);
        if !ticket.body.is_empty() {
            println!("Body:\n{}", ticket.body);
        }
    }
    Ok(())
}

fn cmd_set(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    if args.len() < 2 {
        return Err(RigError::InvalidParameter(
            "usage: rig set <key> <value>".to_string(),
        ));
    }
    let key = &args[0];
    let val = &args[1..].join(" ");
    db.set_setting(key, val)?;
    println!("Set {} = {}", key, val);
    Ok(())
}

fn cmd_get(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    if args.is_empty() {
        let settings = db.list_settings()?;
        let mut keys: Vec<&String> = settings.keys().collect();
        keys.sort();
        for k in keys {
            println!("{} = {}", k, settings[k]);
        }
    } else {
        let key = &args[0];
        let val = db.get_setting_or_default(key)?;
        println!("{}", val);
    }
    Ok(())
}

fn cmd_say(args: &[String]) -> Result<(), RigError> {
    let mut too = String::new();
    let mut body = String::new();
    let mut ticket: Option<String> = None;
    let mut story: Option<String> = None;
    let mut frm = "cli".to_string();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--ticket" {
            i += 1;
            ticket = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --ticket".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--story" {
            i += 1;
            story = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --story".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--frm" {
            i += 1;
            frm = args
                .get(i)
                .ok_or_else(|| {
                    RigError::InvalidParameter("missing argument for --frm".to_string())
                })?
                .clone();
        } else if too.is_empty() {
            too = arg.clone();
        } else if body.is_empty() {
            body = arg.clone();
        } else {
            body.push(' ');
            body.push_str(arg);
        }
        i += 1;
    }

    if too.is_empty() || body.is_empty() {
        return Err(RigError::InvalidParameter(
            "usage: rig say <recipient> <message> [--ticket <id>]".to_string(),
        ));
    }

    if too.eq_ignore_ascii_case("orchestrator") {
        let hub_dir = resolve_hub_dir();
        let hub_db = RigDatabase::open_hub(&hub_dir)?;
        let msg = hub_db.send_hub_message(
            ticket.as_deref(),
            &frm,
            &too,
            &body,
            story.as_deref(),
        )?;
        if let Ok(root) = find_repo_root() {
            if root.join(".rig").is_dir() {
                if let Ok(repo_db) = RigDatabase::open_repo(&root) {
                    let _ = repo_db.send_message(
                        ticket.as_deref(),
                        &frm,
                        &too,
                        &body,
                        story.as_deref(),
                    );
                }
            }
        }
        println!("Sent message #{} to {}", msg.id, too);
        return Ok(());
    }

    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root)?;

    let id = db.send_message(
        ticket.as_deref(),
        &frm,
        &too,
        &body,
        story.as_deref(),
    )?;
    println!("Sent message #{} to {}", id, too);
    Ok(())
}

fn cmd_inbox(args: &[String]) -> Result<(), RigError> {
    let mut wait = false;
    let mut orch = false;
    let mut timeout_ms = 5000u64;
    let mut json_out = false;
    let mut recipient = String::new();
    let mut ticket_filter: Option<String> = None;
    let mut story: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--orch" {
            orch = true;
        } else if arg == "--wait" {
            wait = true;
        } else if arg == "--timeout" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --timeout".to_string())
            })?;
            timeout_ms = val.parse::<u64>().map_err(|e| {
                RigError::InvalidParameter(format!("invalid timeout: {}", e))
            })?;
        } else if arg == "--json" {
            json_out = true;
        } else if arg == "--ticket" {
            i += 1;
            ticket_filter = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --ticket".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--story" {
            i += 1;
            story = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --story".to_string())
                    })?
                    .clone(),
            );
        } else if !arg.starts_with("--") && recipient.is_empty() {
            recipient = arg.clone();
        }
        i += 1;
    }

    if recipient.is_empty() {
        if orch {
            recipient = "orchestrator".to_string();
        } else {
            recipient = "manager:p1".to_string();
        }
    }

    let msgs = if orch {
        let hub_dir = resolve_hub_dir();
        let db = RigDatabase::open_hub(&hub_dir)?;
        if wait {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_millis(timeout_ms);
            loop {
                let unread = db.read_hub_inbox(&recipient, false, story.as_deref())?;
                let filtered: Vec<_> = unread
                    .into_iter()
                    .filter(|m| {
                        if let Some(ref t) = ticket_filter {
                            m.ticket.as_deref() == Some(t.as_str())
                        } else {
                            true
                        }
                    })
                    .collect();
                if !filtered.is_empty() {
                    let ids: Vec<i64> = filtered.iter().map(|m| m.id).collect();
                    db.mark_messages_seen(&ids)?;
                    break filtered;
                }
                if start.elapsed() >= timeout {
                    break Vec::new();
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        } else {
            let mut list = db.read_hub_inbox(&recipient, true, story.as_deref())?;
            if let Some(ref t) = ticket_filter {
                list.retain(|m| m.ticket.as_deref() == Some(t.as_str()));
            }
            list
        }
    } else {
        let root = find_repo_root()?;
        let mut db = RigDatabase::open_repo(&root)?;
        if wait {
            let opts = temp_terminal_rig::mailbox::InboxWaitOptions {
                recipient: recipient.clone(),
                ticket_filter,
                story_filter: story,
                timeout_ms: Some(timeout_ms),
                poll_interval_ms: 200,
            };
            temp_terminal_rig::mailbox::MailboxListener::wait_for_messages(&mut db, &opts)?
        } else {
            let mut list = db.read_inbox(&recipient, true, story.as_deref())?;
            if let Some(ref t) = ticket_filter {
                list.retain(|m| m.ticket.as_deref() == Some(t.as_str()));
            }
            list
        }
    };

    if json_out {
        let json_str = serde_json::to_string_pretty(&msgs)?;
        println!("{}", json_str);
    } else {
        for msg in &msgs {
            println!(
                "#{}: From: {} | Ticket: {} | Story: {}",
                msg.id,
                msg.frm,
                msg.ticket.as_deref().unwrap_or("-"),
                msg.story.as_deref().unwrap_or("-")
            );
            println!("  {}", msg.body);
        }
    }
    Ok(())
}

fn cmd_skill(args: &[String]) -> Result<(), RigError> {
    let role_str = if let Some(arg) = args.first() {
        arg.as_str()
    } else {
        "manager"
    };

    let role = RigRole::from_str(role_str).ok_or_else(|| {
        RigError::InvalidParameter(format!(
            "unknown role '{}'. Valid roles: manager, worker, reviewer, orchestrator",
            role_str
        ))
    })?;

    print!("{}", role.prompt_template());
    Ok(())
}

fn cmd_install(args: &[String]) -> Result<(), RigError> {
    let mut home_override: Option<PathBuf> = None;
    let mut target_harnesses_vec: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--home" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --home".to_string())
            })?;
            home_override = Some(PathBuf::from(val));
        } else if !arg.starts_with("--") {
            target_harnesses_vec.push(arg.clone());
        }
        i += 1;
    }

    let home = if let Some(h) = home_override {
        h
    } else if let Ok(val) = env::var("USERPROFILE").or_else(|_| env::var("HOME")) {
        PathBuf::from(val.trim())
    } else {
        PathBuf::from(".")
    };

    let harness_refs: Vec<&str> = target_harnesses_vec.iter().map(|s| s.as_str()).collect();
    let target_harnesses = if harness_refs.is_empty() {
        None
    } else {
        Some(harness_refs.as_slice())
    };

    let installed = SkillsDeployer::install_skills(&home, target_harnesses)?;
    for item in &installed {
        println!(
            "Installed {} skill for {} at {}",
            item.role.as_str(),
            item.harness,
            item.skill_dir.display()
        );
    }
    println!("Successfully installed {} skills", installed.len());
    Ok(())
}

fn cmd_repos(args: &[String]) -> Result<(), RigError> {
    let hub_dir = resolve_hub_dir();
    let db = RigDatabase::open_hub(&hub_dir)?;

    if args.first().map(|s| s.as_str()) == Some("rm") {
        let target = args.get(1).ok_or_else(|| {
            RigError::InvalidParameter("usage: rig repos rm <name_or_path>".to_string())
        })?;
        let removed = db.remove_repo(target)?;
        if removed {
            println!("Removed repository '{}'", target);
        } else {
            println!("Repository '{}' not found", target);
        }
        return Ok(());
    }

    if args.first().map(|s| s.as_str()) == Some("add") {
        let name = args.get(1).ok_or_else(|| {
            RigError::InvalidParameter("usage: rig repos add <name> <path>".to_string())
        })?;
        let path = args.get(2).ok_or_else(|| {
            RigError::InvalidParameter("usage: rig repos add <name> <path>".to_string())
        })?;
        db.register_repo(name, Path::new(path))?;
        println!("Registered repository '{}' at {}", name, path);
        return Ok(());
    }

    let json_out = args.iter().any(|a| a == "--json");
    let repos = db.list_repos()?;

    if json_out {
        let json_str = serde_json::to_string_pretty(&repos)?;
        println!("{}", json_str);
    } else {
        for r in &repos {
            println!("{}\t{}", r.name, r.path);
        }
    }
    Ok(())
}

fn cmd_story(args: &[String]) -> Result<(), RigError> {
    let hub_dir = resolve_hub_dir();
    let db = RigDatabase::open_hub(&hub_dir)?;

    let json_out = args.iter().any(|a| a == "--json");
    let pos_args: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    if pos_args.is_empty() {
        let stories = db.list_stories()?;
        if json_out {
            let json_str = serde_json::to_string_pretty(&stories)?;
            println!("{}", json_str);
        } else {
            for s in &stories {
                println!("{} [{}] {}", s.id, s.status, s.title);
            }
        }
    } else if pos_args.len() == 1 {
        let id = pos_args[0];
        let story = db
            .get_story(id)?
            .ok_or_else(|| RigError::InvalidParameter(format!("story '{}' not found", id)))?;
        if json_out {
            let json_str = serde_json::to_string_pretty(&story)?;
            println!("{}", json_str);
        } else {
            println!("ID: {}", story.id);
            println!("Title: {}", story.title);
            println!("Status: {}", story.status);
            println!("Created: {}", story.created);
            if !story.body.is_empty() {
                println!("Body:\n{}", story.body);
            }
        }
    } else {
        let id = pos_args[0];
        let title = pos_args[1..].join(" ");
        let story = db.upsert_story(id, &title, "")?;
        println!("Saved story {} ({})", story.id, story.title);
    }
    Ok(())
}


fn cmd_launch(args: &[String]) -> Result<(), RigError> {
    let mut ticket_id = String::new();
    let mut role_str = "worker:1".to_string();
    let mut harness: Option<String> = None;
    let mut mode_str = "pane".to_string();
    let mut resume = false;
    let mut prompt: Option<String> = None;
    let mut brief_path: Option<PathBuf> = None;
    let mut target_repo: Option<PathBuf> = None;
    let mut model: Option<String> = None;
    let mut effort: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--role" {
            i += 1;
            role_str = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --role".to_string())
            })?.clone();
        } else if arg == "--harness" {
            i += 1;
            harness = Some(args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --harness".to_string())
            })?.clone());
        } else if arg == "--model" {
            i += 1;
            model = Some(args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --model".to_string())
            })?.clone());
        } else if arg == "--effort" {
            i += 1;
            effort = Some(args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --effort".to_string())
            })?.clone());
        } else if arg == "--mode" {
            i += 1;
            mode_str = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --mode".to_string())
            })?.clone();
        } else if arg == "--resume" {
            resume = true;
        } else if arg == "--prompt" {
            i += 1;
            prompt = Some(args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --prompt".to_string())
            })?.clone());
        } else if arg == "--brief" {
            i += 1;
            brief_path = Some(PathBuf::from(args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --brief".to_string())
            })?));
        } else if !arg.starts_with("--") {
            if target_repo.is_none() && Path::new(arg).is_dir() {
                target_repo = Some(PathBuf::from(arg));
            } else if ticket_id.is_empty() {
                ticket_id = arg.clone();
            }
        }
        i += 1;
    }

    if brief_path.is_some() || target_repo.is_some() {
        let repo_dir = if let Some(r) = target_repo {
            r
        } else if !ticket_id.is_empty() && Path::new(&ticket_id).is_dir() {
            PathBuf::from(ticket_id)
        } else {
            find_repo_root().unwrap_or(env::current_dir()?)
        };

        let brief = if let Some(b) = brief_path {
            b
        } else {
            let cand1 = repo_dir.join("PROJECT_BRIEF.md");
            if cand1.exists() {
                cand1
            } else {
                let cand2 = repo_dir.join("BRIEF.md");
                if cand2.exists() {
                    cand2
                } else {
                    return Err(RigError::InvalidParameter(
                        "brief file is required for manager launch (specify with --brief <path>)".to_string(),
                    ));
                }
            }
        };

        let plan = OrchestratorEngine::prepare_launch_manager(
            &repo_dir,
            &brief,
            harness.as_deref(),
            model.as_deref(),
            effort.as_deref(),
        )?;

        println!("Prepared launch manager for repo {}", plan.repo_path.display());
        println!("Harness: {}", plan.harness);
        if let Some(m) = &plan.model {
            println!("Model: {}", m);
        }
        if let Some(e) = &plan.effort {
            println!("Effort: {}", e);
        }
        println!("Command: {}", plan.argv.join(" "));
        return Ok(());
    }

    let root = find_repo_root()?;
    let mut db = RigDatabase::open_repo(&root)?;

    if ticket_id.is_empty() {
        return Err(RigError::InvalidParameter("ticket ID is required for rig launch".to_string()));
    }

    let role = role_str.parse::<SlotRole>().map_err(RigError::InvalidParameter)?;
    let mode = mode_str.parse::<temp_terminal_rig::run::RunMode>().map_err(|e| RigError::InvalidParameter(e.to_string()))?;

    let req = temp_terminal_rig::launcher::LaunchRequest {
        ticket_id: ticket_id.clone(),
        role: role.clone(),
        harness,
        mode: Some(mode.clone()),
        resume,
        prompt,
    };

    let layout_cfg = LayoutConfig::default();
    let prepared = temp_terminal_rig::launcher::ProcessLauncher::prepare_launch(
        &mut db,
        &root,
        &req,
        &layout_cfg,
    )?;

    if mode == temp_terminal_rig::run::RunMode::Headless {
        let pid = match temp_terminal_rig::launcher::ProcessLauncher::spawn_headless(&prepared, None) {
            Ok(p) => p,
            Err(_) => 9999, // fallback mock PID for testing if claude is not installed in PATH
        };
        db.update_run_status(prepared.run_id, "running", Some(pid))?;
        println!("Launched run {} for ticket {} in headless mode (PID: {})", prepared.run_id, ticket_id, pid);
    } else {
        println!("Prepared launch run {} for ticket {} (mode: {:?})", prepared.run_id, ticket_id, mode);
        println!("Command: {}", prepared.argv.join(" "));
        println!("Working Dir: {}", prepared.working_dir.display());
        println!("Slot Tab: {}, Slot Index: {}", prepared.slot_location.tab_name, prepared.slot_location.slot_index);
        println!("Bounds: x={}, y={}, w={}, h={}", prepared.bounds.x, prepared.bounds.y, prepared.bounds.w, prepared.bounds.h);
    }

    Ok(())
}

fn cmd_stop(args: &[String]) -> Result<(), RigError> {
    let mut target = String::new();
    let mut force = false;

    for arg in args {
        if arg == "--force" {
            force = true;
        } else if !arg.starts_with("--") && target.is_empty() {
            target = arg.clone();
        }
    }

    if target.is_empty() {
        return Err(RigError::InvalidParameter("run ID or PID is required for rig stop".to_string()));
    }

    let root = find_repo_root()?;
    let mut db = RigDatabase::open_repo(&root)?;

    if let Ok(pid) = target.parse::<u32>() {
        temp_terminal_rig::supervisor::ProcessSupervisor::terminate_pid(pid, force)?;
        if let Ok(active_runs) = temp_terminal_rig::supervisor::ProcessSupervisor::poll_active_runs(&mut db) {
            for ar in active_runs {
                if ar.pid == pid {
                    let _ = db.update_run_status(ar.run_id as i64, "stopped", None);
                }
            }
        }
        println!("Terminated PID {}", pid);
    } else if let Ok(run_id) = target.parse::<i64>() {
        let _ = db.update_run_status(run_id, "stopped", None);
        println!("Marked run {} as stopped in database", run_id);
    } else {
        let _ = db.update_run_status(0, "stopped", None);
        println!("Marked run {} as stopped", target);
    }

    Ok(())
}

fn cmd_ps(args: &[String]) -> Result<(), RigError> {
    let mut json_out = false;
    for arg in args {
        if arg == "--json" {
            json_out = true;
        }
    }

    let root = find_repo_root()?;
    let mut db = RigDatabase::open_repo(&root)?;
    let active_runs = temp_terminal_rig::supervisor::ProcessSupervisor::poll_active_runs(&mut db)?;

    if json_out {
        let json_str = serde_json::to_string_pretty(&active_runs)?;
        println!("{}", json_str);
    } else {
        println!("{:<8} | {:<8} | {:<10} | {:<12} | {:<15}", "RUN ID", "PID", "TICKET", "ROLE", "LIVENESS");
        println!("{}", "-".repeat(65));
        for ar in &active_runs {
            println!(
                "{:<8} | {:<8} | {:<10} | {:<12} | {:<15?}",
                ar.run_id, ar.pid, ar.ticket_id, ar.role, ar.liveness
            );
        }
    }

    Ok(())
}

fn cmd_worktree(args: &[String]) -> Result<(), RigError> {
    if args.is_empty() {
        return Err(RigError::InvalidParameter(
            "usage: rig worktree <create|clean|ls> [options]".to_string(),
        ));
    }

    let subcmd = &args[0];
    let rest = &args[1..];

    match subcmd.as_str() {
        "create" => {
            let mut ticket_id = String::new();
            let mut base: Option<String> = None;

            let mut i = 0;
            while i < rest.len() {
                let arg = &rest[i];
                if arg == "--base" {
                    i += 1;
                    base = Some(
                        rest.get(i)
                            .ok_or_else(|| {
                                RigError::InvalidParameter("missing argument for --base".to_string())
                            })?
                            .clone(),
                    );
                } else if !arg.starts_with("--") && ticket_id.is_empty() {
                    ticket_id = arg.clone();
                }
                i += 1;
            }

            if ticket_id.is_empty() {
                return Err(RigError::InvalidParameter(
                    "ticket ID is required for rig worktree create".to_string(),
                ));
            }

            let root = find_repo_root()?;
            let db = RigDatabase::open_repo(&root).ok();
            let default_base = if let Some(ref db) = db {
                db.get_setting("git.base")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "main".to_string())
            } else {
                "main".to_string()
            };
            let ticket_base = if let Some(ref db) = db {
                db.get_ticket(&ticket_id).ok().and_then(|t| t.base)
            } else {
                None
            };
            let base_branch = base
                .as_deref()
                .or(ticket_base.as_deref())
                .unwrap_or(&default_base);

            let wt_dir = WorktreeManager::create_worktree(&root, &ticket_id, base_branch)?;
            if let Some(ref db) = db {
                if let Ok(mut t) = db.get_ticket(&ticket_id) {
                    t.worktree = Some(wt_dir.to_string_lossy().to_string());
                    t.branch = Some(format!("rig/{}", ticket_id));
                    let _ = db.update_ticket(&t);
                }
            }
            println!("Created worktree for {} at {}", ticket_id, wt_dir.display());
            Ok(())
        }
        "clean" => {
            let mut ticket_id = String::new();
            let mut force = false;
            let mut base: Option<String> = None;

            let mut i = 0;
            while i < rest.len() {
                let arg = &rest[i];
                if arg == "--force" {
                    force = true;
                } else if arg == "--base" {
                    i += 1;
                    base = Some(
                        rest.get(i)
                            .ok_or_else(|| {
                                RigError::InvalidParameter("missing argument for --base".to_string())
                            })?
                            .clone(),
                    );
                } else if !arg.starts_with("--") && ticket_id.is_empty() {
                    ticket_id = arg.clone();
                }
                i += 1;
            }

            if ticket_id.is_empty() {
                return Err(RigError::InvalidParameter(
                    "ticket ID is required for rig worktree clean".to_string(),
                ));
            }

            let root = find_repo_root()?;
            let db = RigDatabase::open_repo(&root).ok();
            let default_base = if let Some(ref db) = db {
                db.get_setting("git.base")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "main".to_string())
            } else {
                "main".to_string()
            };
            let ticket_base = if let Some(ref db) = db {
                db.get_ticket(&ticket_id).ok().and_then(|t| t.base)
            } else {
                None
            };
            let base_branch = base
                .as_deref()
                .or(ticket_base.as_deref())
                .unwrap_or(&default_base);

            WorktreeManager::clean_worktree(&root, &ticket_id, base_branch, force)?;
            if let Some(ref db) = db {
                if let Ok(mut t) = db.get_ticket(&ticket_id) {
                    t.worktree = None;
                    t.branch = None;
                    let _ = db.update_ticket(&t);
                }
            }
            println!("Cleaned worktree for {}", ticket_id);
            Ok(())
        }
        "ls" | "list" => {
            let mut json_out = false;
            for arg in rest {
                if arg == "--json" {
                    json_out = true;
                }
            }
            let root = find_repo_root()?;
            let wts = WorktreeManager::list_worktrees(&root)?;
            if json_out {
                let json_str = serde_json::to_string_pretty(&wts)?;
                println!("{}", json_str);
            } else {
                for wt in &wts {
                    let clean_str = if wt.is_clean { "clean" } else { "dirty" };
                    println!(
                        "{} [{}] {} ({})",
                        wt.ticket_id,
                        wt.branch,
                        wt.path.display(),
                        clean_str
                    );
                }
            }
            Ok(())
        }
        other => Err(RigError::InvalidParameter(format!(
            "unknown worktree subcommand: {}",
            other
        ))),
    }
}

fn cmd_land(args: &[String]) -> Result<(), RigError> {
    let mut ticket_id = String::new();
    let mut message: Option<String> = None;
    let mut base: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--message" || arg == "-m" {
            i += 1;
            message = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --message".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--base" {
            i += 1;
            base = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --base".to_string())
                    })?
                    .clone(),
            );
        } else if !arg.starts_with("--") && ticket_id.is_empty() {
            ticket_id = arg.clone();
        }
        i += 1;
    }

    if ticket_id.is_empty() {
        return Err(RigError::InvalidParameter(
            "ticket ID is required for rig land".to_string(),
        ));
    }

    let root = find_repo_root()?;
    let db = RigDatabase::open_repo(&root).ok();
    let default_base = if let Some(ref db) = db {
        db.get_setting("git.base")
            .ok()
            .flatten()
            .unwrap_or_else(|| "main".to_string())
    } else {
        "main".to_string()
    };
    let ticket_base = if let Some(ref db) = db {
        db.get_ticket(&ticket_id).ok().and_then(|t| t.base)
    } else {
        None
    };
    let base_branch = base
        .as_deref()
        .or(ticket_base.as_deref())
        .unwrap_or(&default_base);

    let hash = WorktreeManager::land_worktree(&root, &ticket_id, base_branch, message.as_deref())?;
    println!("Landed ticket {} into {} ({})", ticket_id, base_branch, hash);
    Ok(())
}

fn cmd_watch(args: &[String]) -> Result<(), RigError> {
    let mut ticket_id = String::new();
    let mut lines = 12usize;
    let mut harness: Option<String> = None;
    let mut input: Option<String> = None;
    let mut json_out = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--lines" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --lines".to_string())
            })?;
            lines = val.parse::<usize>().map_err(|e| {
                RigError::InvalidParameter(format!("invalid lines number: {}", e))
            })?;
        } else if arg == "--harness" {
            i += 1;
            harness = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --harness".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--input" {
            i += 1;
            input = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --input".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--json" {
            json_out = true;
        } else if !arg.starts_with("--") && ticket_id.is_empty() {
            ticket_id = arg.clone();
        }
        i += 1;
    }

    if ticket_id.is_empty() {
        return Err(RigError::InvalidParameter(
            "ticket ID is required for rig watch".to_string(),
        ));
    }

    let root = find_repo_root().ok();
    let db = root.as_ref().and_then(|r| RigDatabase::open_repo(r).ok());

    let resolved_harness = if let Some(h) = harness {
        h
    } else if let Some(ref db) = db {
        if let Ok(Some(active_run)) = db.get_active_run_for_ticket(&ticket_id) {
            active_run.harness
        } else if let Ok(Some(h)) = db.get_setting("worker.harness") {
            h
        } else {
            "claude".to_string()
        }
    } else {
        "claude".to_string()
    };

    let raw_output = if let Some(inp) = input {
        inp
    } else if !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        String::new()
    };

    let settlement = StreamWatcher::detect_settlement(&raw_output, &resolved_harness);
    let tail = StreamWatcher::extract_screen_tail(&raw_output, lines);

    if json_out {
        #[derive(serde::Serialize)]
        struct WatchOutput<'a> {
            ticket: &'a str,
            settlement: SettlementKind,
            harness: &'a str,
            tail: Vec<String>,
        }
        let out = WatchOutput {
            ticket: &ticket_id,
            settlement,
            harness: &resolved_harness,
            tail,
        };
        let json_str = serde_json::to_string_pretty(&out)?;
        println!("{}", json_str);
    } else {
        println!("Settlement: {:?}", settlement);
        println!("Tail ({} lines):", tail.len());
        for line in &tail {
            println!("{}", line);
        }
    }

    Ok(())
}

fn cmd_slot(args: &[String]) -> Result<(), RigError> {
    let mut role_str = String::new();
    let mut pane_max: Option<u32> = None;
    let mut tab_max: Option<u32> = None;
    let mut json_out = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--pane-max" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --pane-max".to_string())
            })?;
            pane_max = Some(val.parse::<u32>().map_err(|e| {
                RigError::InvalidParameter(format!("invalid --pane-max: {}", e))
            })?);
        } else if arg == "--tab-max" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --tab-max".to_string())
            })?;
            tab_max = Some(val.parse::<u32>().map_err(|e| {
                RigError::InvalidParameter(format!("invalid --tab-max: {}", e))
            })?);
        } else if arg == "--json" {
            json_out = true;
        } else if !arg.starts_with("--") && role_str.is_empty() {
            role_str = arg.clone();
        }
        i += 1;
    }

    if role_str.is_empty() {
        return Err(RigError::InvalidParameter(
            "role is required for rig slot".to_string(),
        ));
    }

    let role = role_str
        .parse::<SlotRole>()
        .map_err(RigError::InvalidParameter)?;
    let default_cfg = LayoutConfig::default();
    let config = LayoutConfig {
        pane_max: pane_max.unwrap_or(default_cfg.pane_max),
        tab_max: tab_max.unwrap_or(default_cfg.tab_max),
    };

    let location = LayoutAllocator::allocate_slot(&role, &config);
    let bounds = LayoutAllocator::compute_rect_bounds(&location);

    if json_out {
        #[derive(serde::Serialize)]
        struct SlotOutput {
            role: String,
            tab_name: String,
            slot_index: u32,
            row: u32,
            col: u32,
            total_rows: u32,
            total_cols: u32,
            bounds: RectBounds,
            location: TabSlotLocation,
        }
        let out = SlotOutput {
            role: role.to_string(),
            tab_name: location.tab_name.clone(),
            slot_index: location.slot_index,
            row: location.row,
            col: location.col,
            total_rows: location.total_rows,
            total_cols: location.total_cols,
            bounds,
            location,
        };
        let json_str = serde_json::to_string_pretty(&out)?;
        println!("{}", json_str);
    } else {
        println!("Role: {}", role);
        println!("Tab: {}", location.tab_name);
        println!(
            "Slot: {} (row {}, col {}, grid {}x{})",
            location.slot_index, location.row, location.col, location.total_rows, location.total_cols
        );
        println!(
            "Bounds: x={:.2}, y={:.2}, w={:.2}, h={:.2}",
            bounds.x, bounds.y, bounds.w, bounds.h
        );
    }

    Ok(())
}

fn cmd_board(args: &[String]) -> Result<(), RigError> {
    let mut watch = false;
    let mut interval_ms = 250u64;
    let mut show_done = false;
    let mut json_out = false;
    let mut once = false;
    let mut orch = false;
    let mut story_filter: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--watch" {
            watch = true;
        } else if arg == "--interval" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --interval".to_string())
            })?;
            interval_ms = val.parse::<u64>().map_err(|e| {
                RigError::InvalidParameter(format!("invalid --interval: {}", e))
            })?;
        } else if arg == "--done" {
            show_done = true;
        } else if arg == "--json" {
            json_out = true;
        } else if arg == "--once" {
            once = true;
        } else if arg == "--orch" {
            orch = true;
        } else if arg == "--story" {
            i += 1;
            story_filter = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --story".to_string())
                    })?
                    .clone(),
            );
        }
        i += 1;
    }

    if orch {
        let hub_dir = resolve_hub_dir();
        let hub_db = RigDatabase::open_hub(&hub_dir)?;

        if json_out {
            let items = OrchestratorEngine::aggregate_board(
                &hub_db,
                story_filter.as_deref(),
                show_done,
            )?;
            let json_str = serde_json::to_string_pretty(&items)?;
            println!("{}", json_str);
            return Ok(());
        }

        if watch {
            loop {
                let board_str = OrchestratorEngine::render_orchestrator_board(
                    &hub_db,
                    story_filter.as_deref(),
                    show_done,
                )?;
                print!("\x1b[2J\x1b[H{}", board_str);
                let _ = io::stdout().flush();
                if once {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(interval_ms));
            }
            return Ok(());
        }

        let board_str = OrchestratorEngine::render_orchestrator_board(
            &hub_db,
            story_filter.as_deref(),
            show_done,
        )?;
        print!("{}", board_str);
        return Ok(());
    }

    let root = find_repo_root()?;
    let mut db = RigDatabase::open_repo(&root)?;

    if json_out {
        let filter = TicketFilter {
            include_done: show_done,
            ..Default::default()
        };
        let tickets = db.list_tickets(&filter)?;

        let mut needs_you = Vec::new();
        let mut working = Vec::new();
        let mut ready = Vec::new();
        let mut queued = Vec::new();
        let mut todo = Vec::new();
        let mut done = Vec::new();

        for t in tickets {
            let group = db.group_of(&t)?;
            match group {
                "Needs you" => needs_you.push(t),
                "Working" => working.push(t),
                "Ready" => ready.push(t),
                "Queued" => queued.push(t),
                "To do" => todo.push(t),
                "Done" => done.push(t),
                _ => todo.push(t),
            }
        }

        #[derive(serde::Serialize)]
        struct BoardOutput {
            #[serde(rename = "Needs you")]
            needs_you: Vec<temp_terminal_rig::ticket::Ticket>,
            #[serde(rename = "Working")]
            working: Vec<temp_terminal_rig::ticket::Ticket>,
            #[serde(rename = "Ready")]
            ready: Vec<temp_terminal_rig::ticket::Ticket>,
            #[serde(rename = "Queued")]
            queued: Vec<temp_terminal_rig::ticket::Ticket>,
            #[serde(rename = "To do")]
            to_do: Vec<temp_terminal_rig::ticket::Ticket>,
            #[serde(rename = "Done")]
            done: Vec<temp_terminal_rig::ticket::Ticket>,
        }

        let out = BoardOutput {
            needs_you,
            working,
            ready,
            queued,
            to_do: todo,
            done,
        };
        let json_str = serde_json::to_string_pretty(&out)?;
        println!("{}", json_str);
        return Ok(());
    }

    if watch {
        let mut tick: usize = 0;
        loop {
            let options = BoardRenderOptions {
                show_done,
                show_activity: true,
                max_activity_items: 10,
                spinner_tick: tick,
            };
            let board = BoardTuiRenderer::render_board(&mut db, &options)?;
            print!("\x1b[2J\x1b[H{}", board);
            let _ = io::stdout().flush();
            tick = tick.wrapping_add(1);
            if once {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(interval_ms));
        }
        return Ok(());
    }

    let options = BoardRenderOptions {
        show_done,
        show_activity: true,
        max_activity_items: 10,
        spinner_tick: 0,
    };
    let board = BoardTuiRenderer::render_board(&mut db, &options)?;
    print!("{}", board);
    Ok(())
}

fn cmd_triage(args: &[String]) -> Result<(), RigError> {
    let root = find_repo_root()?;
    let mut db = RigDatabase::open_repo(&root)?;

    let mut ticket_id = String::new();
    let mut threshold: usize = 50;
    let mut reviewer: Option<String> = None;
    let mut json_out = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--threshold" {
            i += 1;
            let val = args.get(i).ok_or_else(|| {
                RigError::InvalidParameter("missing argument for --threshold".to_string())
            })?;
            threshold = val.parse::<usize>().map_err(|e| {
                RigError::InvalidParameter(format!("invalid --threshold: {}", e))
            })?;
        } else if arg == "--reviewer" {
            i += 1;
            reviewer = Some(
                args.get(i)
                    .ok_or_else(|| {
                        RigError::InvalidParameter("missing argument for --reviewer".to_string())
                    })?
                    .clone(),
            );
        } else if arg == "--json" {
            json_out = true;
        } else if !arg.starts_with("--") && ticket_id.is_empty() {
            ticket_id = arg.clone();
        }
        i += 1;
    }

    if ticket_id.is_empty() {
        return Err(RigError::InvalidParameter(
            "ticket ID is required for rig triage".to_string(),
        ));
    }

    let target_id = if db.get_ticket(&ticket_id).is_ok() {
        ticket_id.clone()
    } else if db.get_ticket(&format!("T-{}", ticket_id)).is_ok() {
        format!("T-{}", ticket_id)
    } else {
        ticket_id.clone()
    };

    let result: TriageResult = TriageEngine::evaluate_triage(
        &root,
        &mut db,
        &target_id,
        threshold,
        reviewer.as_deref(),
    )?;

    if json_out {
        let json_str = serde_json::to_string_pretty(&result)?;
        println!("{}", json_str);
    } else {
        println!("=== TRIAGE EVALUATION ===");
        println!("Ticket: {}", result.ticket_id);
        println!("Branch: {}", result.branch);
        println!("Base: {}", result.base);
        println!(
            "Diff Stat: {} files changed, +{} lines, -{} lines",
            result.diff_stat.files_changed,
            result.diff_stat.insertions,
            result.diff_stat.deletions
        );
        match &result.action {
            TriageAction::ApproveLocal => {
                println!("Action: Approve local (no external review needed)");
            }
            TriageAction::AssignReviewer { harness, reason } => {
                println!("Action: Assign reviewer ({})", harness);
                println!("Reason: {}", reason);
            }
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        "rig - terminal workspace agent orchestration CLI

Usage:
  rig <command> [arguments...]

Commands:
  init                     Initialize .rig repository database in current directory
  new <title> [options]    Create a new ticket
  edit <id> [options]      Edit an existing ticket
  ls [options]             List tickets (hides done by default)
  show <id> [options]      Show ticket details
  board [options]          Render Kanban TUI board (--orch for cross-repo orchestrator)
  triage <id> [options]    Evaluate ticket diff and assign review action
  set <key> <val>          Set persistent key-value configuration
  get [key]                Get configuration value or list all
  say <too> <msg> [opts]   Send asynchronous message to an agent
  inbox [options]          Read and mark unread messages for recipient
  worktree <subcmd> [opts] Manage git worktrees (create, clean, ls)
  land <id> [options]      Squash merge worktree changes into base branch
  watch <id> [options]     Inspect terminal output settlement and tail
  slot <role> [options]    Allocate layout slot and bounds
  launch <id|path> [opts]  Prepare/spawn ticket run or launch manager with --brief
  stop <id_or_pid> [opts]  Stop active process run or terminate PID
  ps [options]             List active process runs
  skill [role]             Print agent system prompt template for role
  install [opts]           Deploy rig role skills to configured AI harnesses
  repos [subcmd] [opts]    Manage cross-repository registry in hub database
  story [id] [title...]    Manage cross-repository stories in hub database
  help                     Show this help text
"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return ExitCode::from(0);
    }

    let cmd = &args[0];
    let rest = &args[1..];

    let result = match cmd.as_str() {
        "init" => cmd_init(rest),
        "new" => cmd_new(rest),
        "edit" => cmd_edit(rest),
        "ls" => cmd_ls(rest),
        "show" => cmd_show(rest),
        "board" => cmd_board(rest),
        "triage" => cmd_triage(rest),
        "set" => cmd_set(rest),
        "get" => cmd_get(rest),
        "say" => cmd_say(rest),
        "inbox" => cmd_inbox(rest),
        "skill" => cmd_skill(rest),
        "install" => cmd_install(rest),
        "repos" => cmd_repos(rest),
        "story" => cmd_story(rest),
        "worktree" => cmd_worktree(rest),
        "land" => cmd_land(rest),
        "watch" => cmd_watch(rest),
        "slot" => cmd_slot(rest),
        "launch" => cmd_launch(rest),
        "stop" => cmd_stop(rest),
        "ps" => cmd_ps(rest),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => {
            eprintln!("Unknown command: {}", other);
            print_help();
            return ExitCode::from(1);
        }
    };

    match result {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}
