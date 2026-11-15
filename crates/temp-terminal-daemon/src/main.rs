//! Background daemon runner and server binary for temp-terminal.
//! Handles startup options, IPC address resolution, log routing, and shutdown signaling.

use std::env;
use std::process::ExitCode;
use temp_terminal_core::paths;
use temp_terminal_daemon::{DaemonOptions, DaemonServer};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let bind_addr = env::var("TEMPTERM_ADDR")
        .or_else(|_| env::var("TEMP_TERMINAL_ADDR"))
        .or_else(|_| env::var("OMATERM_ADDR"))
        .unwrap_or_else(|_| {
            let mut addr = paths::default_daemon_addr();
            // Check for --addr or -a argument
            for i in 0..args.len() {
                if (args[i] == "--addr" || args[i] == "-a") && i + 1 < args.len() {
                    addr = args[i + 1].clone();
                    break;
                }
            }
            addr
        });

    let proxy_addr = env::var("TEMPTERM_PROXY_ADDR")
        .or_else(|_| env::var("TEMP_TERMINAL_PROXY_ADDR"))
        .or_else(|_| env::var("OMATERM_PROXY_ADDR"))
        .ok();

    if let Ok(parsed_addr) = bind_addr.parse::<std::net::SocketAddr>() {
        if !parsed_addr.ip().is_loopback() {
            eprintln!(
                "WARNING [SECURITY]: temp-terminal daemon bound to non-loopback address {}. \
                External network access is enabled. Sensitive commands like daemon.shutdown \
                will require loopback origin.",
                bind_addr
            );
        }
    }

    println!("Starting temp-terminal daemon on {}...", bind_addr);
    let opts = DaemonOptions {
        bind_addr,
        proxy_addr,
        app_name: paths::app_name().to_string(),
    };

    let server = match DaemonServer::start(opts).await {
        Ok(s) => s,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                eprintln!(
                    "Failed to start temp-terminal daemon: address is already in use by another instance."
                );
            } else {
                eprintln!("Failed to start temp-terminal daemon: {}", e);
            }
            return ExitCode::from(1);
        }
    };

    println!("temp-terminal daemon running on {}", server.addr());

    // Wait for shutdown signal (Ctrl+C or SIGTERM)
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Received termination signal, shutting down daemon...");
        }
        _ = server.wait_shutdown() => {
            println!("Received remote shutdown request, shutting down daemon...");
        }
    }

    server.close().await;
    println!("temp-terminal daemon stopped cleanly.");
    ExitCode::from(0)
}
