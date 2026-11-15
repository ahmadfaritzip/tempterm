//! Main CLI entry point.
//! Exit status codes matching Go `tempterm/internal/cli/cli.go`:
//! 0 ok, 1 failed, 2 bad usage, 3 no such target, 4 unreachable.

pub mod client;
pub mod cmd;

use std::env;
use std::process::ExitCode;

use client::{Client, ClientError};
use cmd::VERSION;

fn print_help() {
    println!(
        "temp-terminal {} - modern terminal workspace & agent supervisor

Usage:
  temp-terminal <command> [arguments...]

Commands:
  ps             List processes for the active project
  start <name>   Start a project process (--wait, -t 60s)
  stop <name>    Stop a running process
  restart <name> Restart a process (--wait, -t 60s)
  trust <name>   Mark a process definition as trusted
  ask <target>   Send a message to an agent (--no-wait, -t timeout)
  reply [id]     Reply to an incoming agent message
  inbox          Show messages in pane inbox (--json)
  ping           Ping running daemon
  shutdown       Gracefully shut down running daemon
  version        Print CLI version
  help           Show this help text
",
        VERSION
    );
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return ExitCode::from(0);
    }

    let subcmd = args[0].as_str();
    let rest = &args[1..];

    match subcmd {
        "help" | "-h" | "--help" => {
            print_help();
            return ExitCode::from(0);
        }
        "version" | "-v" | "--version" => {
            println!("temp-terminal {}", VERSION);
            return ExitCode::from(0);
        }
        _ => {}
    }

    let daemon_addr = temp_terminal_core::paths::default_daemon_addr();

    let mut client = match Client::connect(&daemon_addr).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("temp-terminal: {}", e);
            return ExitCode::from(match e {
                ClientError::Unreachable(_) => 4,
                ClientError::Usage(_) => 2,
                ClientError::NotFound(_) => 3,
                _ => 1,
            });
        }
    };

    let result = match subcmd {
        "ps" => cmd::run_ps(&mut client, rest).await,
        "start" | "stop" | "restart" | "trust" => {
            cmd::run_process_cmd(&mut client, subcmd, rest).await
        }
        "ask" => cmd::run_ask(&mut client, rest).await,
        "reply" => cmd::run_reply(&mut client, rest).await,
        "inbox" => cmd::run_inbox(&mut client, rest).await,
        "ping" | "shutdown" => cmd::run_daemon_cmd(&mut client, subcmd).await,
        unknown => {
            eprintln!("temp-terminal: unknown command {}", unknown);
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("temp-terminal: {}", e);
            ExitCode::from(match e {
                ClientError::Usage(_) => 2,
                ClientError::NotFound(_) => 3,
                ClientError::Unreachable(_) => 4,
                _ => 1,
            })
        }
    }
}
