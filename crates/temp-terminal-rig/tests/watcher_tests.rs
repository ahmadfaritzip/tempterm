use temp_terminal_rig::watcher::{SettlementKind, StreamWatcher};

#[test]
fn test_strip_ansi_colors_and_styles() {
    let raw = "\x1b[31mError:\x1b[0m \x1b[1;32mEverything is fine\x1b[0m";
    let cleaned = StreamWatcher::strip_ansi(raw);
    assert_eq!(cleaned, "Error: Everything is fine");

    let raw_256 = "\x1b[38;5;201mPurple\x1b[0m and \x1b[48;5;28mGreenBG\x1b[0m";
    assert_eq!(StreamWatcher::strip_ansi(raw_256), "Purple and GreenBG");
}

#[test]
fn test_strip_ansi_cursor_and_screen_controls() {
    let raw = "\x1b[2J\x1b[H\x1b[?25lLoading...\x1b[?25h\x1b[KDone";
    let cleaned = StreamWatcher::strip_ansi(raw);
    assert_eq!(cleaned, "Loading...Done");
}

#[test]
fn test_strip_ansi_osc_sequences() {
    let osc_bel = "\x1b]0;Agent Working Window\x07Output line 1\nOutput line 2";
    assert_eq!(
        StreamWatcher::strip_ansi(osc_bel),
        "Output line 1\nOutput line 2"
    );

    let osc_st = "\x1b]2;Codex Session\x1b\\Result ready";
    assert_eq!(StreamWatcher::strip_ansi(osc_st), "Result ready");
}

#[test]
fn test_extract_screen_tail_exactly_twelve_lines() {
    let mut lines = Vec::new();
    for i in 1..=20 {
        lines.push(format!("Line {:02}", i));
    }
    let raw = lines.join("\n");

    let tail = StreamWatcher::extract_screen_tail(&raw, 12);
    assert_eq!(tail.len(), 12);
    assert_eq!(tail[0], "Line 09");
    assert_eq!(tail[11], "Line 20");
}

#[test]
fn test_extract_screen_tail_strips_trailing_empty_lines() {
    let raw = "Header\nMiddle line 1\nMiddle line 2\nTail line\n   \n\t\n\n";
    let tail = StreamWatcher::extract_screen_tail(raw, 12);
    assert_eq!(tail.len(), 4);
    assert_eq!(tail[0], "Header");
    assert_eq!(tail[3], "Tail line");
}

#[test]
fn test_extract_screen_tail_fewer_than_requested() {
    let raw = "Single line output";
    let tail = StreamWatcher::extract_screen_tail(raw, 12);
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0], "Single line output");

    let empty_tail = StreamWatcher::extract_screen_tail("", 12);
    assert!(empty_tail.is_empty());
}

#[test]
fn test_extract_screen_tail_strips_ansi_codes() {
    let raw = "\x1b[32mSuccess line 1\x1b[0m\n\x1b[33mWarning line 2\x1b[0m";
    let tail = StreamWatcher::extract_screen_tail(raw, 12);
    assert_eq!(tail.len(), 2);
    assert_eq!(tail[0], "Success line 1");
    assert_eq!(tail[1], "Warning line 2");
}

#[test]
fn test_detect_settlement_idle_claude() {
    let output = "Analyzing repository...\nTool call completed.\n\x1b[32m❯\x1b[0m ";
    assert_eq!(
        StreamWatcher::detect_settlement(output, "claude"),
        SettlementKind::Idle
    );

    let plain_claude = "Done.\n❯ ";
    assert_eq!(
        StreamWatcher::detect_settlement(plain_claude, "claude"),
        SettlementKind::Idle
    );

    let boxed_claude = "Tips: Use /help\n╭───\n╰─── ❯ ";
    assert_eq!(
        StreamWatcher::detect_settlement(boxed_claude, "claude"),
        SettlementKind::Idle
    );
}

#[test]
fn test_detect_settlement_idle_codex() {
    let output = "Searching files...\nFound 3 results.\n› ";
    assert_eq!(
        StreamWatcher::detect_settlement(output, "codex"),
        SettlementKind::Idle
    );

    let colored_codex = "Ready.\n\x1b[34m›\x1b[0m ";
    assert_eq!(
        StreamWatcher::detect_settlement(colored_codex, "codex"),
        SettlementKind::Idle
    );
}

#[test]
fn test_detect_settlement_idle_generic_and_shells() {
    let bash_output = "user@box:~$ ";
    assert_eq!(
        StreamWatcher::detect_settlement(bash_output, "bash"),
        SettlementKind::Idle
    );

    let generic_output = "build finished\n$ ";
    assert_eq!(
        StreamWatcher::detect_settlement(generic_output, "generic"),
        SettlementKind::Idle
    );

    let powershell_output = "Windows PowerShell\nPS C:\\Users\\Asus> ";
    assert_eq!(
        StreamWatcher::detect_settlement(powershell_output, "generic"),
        SettlementKind::Idle
    );

    let pi_output = "pi> ";
    assert_eq!(
        StreamWatcher::detect_settlement(pi_output, "pi"),
        SettlementKind::Idle
    );

    let opencode_output = "opencode> ";
    assert_eq!(
        StreamWatcher::detect_settlement(opencode_output, "opencode"),
        SettlementKind::Idle
    );

    let omp_output = "repo on main \x1b[32m❯\x1b[0m ";
    assert_eq!(
        StreamWatcher::detect_settlement(omp_output, "omp"),
        SettlementKind::Idle
    );
}

#[test]
fn test_detect_settlement_blocked_confirmation() {
    let prompt_yn = "File already exists. Overwrite? [y/N] ";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_yn, "claude"),
        SettlementKind::Blocked
    );

    let prompt_paren_yn = "Delete branch? (y/n): ";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_paren_yn, "generic"),
        SettlementKind::Blocked
    );

    let prompt_perm = "Security Alert:\nPermission required to execute shell command.";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_perm, "claude"),
        SettlementKind::Blocked
    );

    let prompt_allow = "Allow this action? [Y/n] ";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_allow, "codex"),
        SettlementKind::Blocked
    );

    let prompt_continue = "Do you want to continue? [yes/no] ";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_continue, "generic"),
        SettlementKind::Blocked
    );

    let prompt_password = "[sudo] password for user: ";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_password, "generic"),
        SettlementKind::Blocked
    );

    let prompt_passphrase = "Enter passphrase for key '/id_rsa': ";
    assert_eq!(
        StreamWatcher::detect_settlement(prompt_passphrase, "claude"),
        SettlementKind::Blocked
    );
}

#[test]
fn test_detect_settlement_historical_prompt_not_blocked() {
    let output = "Do you want to continue? [yes/no] y\nDownloading packages...\nCompiling temp-terminal v0.1.0\nReady.\n❯ ";
    assert_eq!(
        StreamWatcher::detect_settlement(output, "claude"),
        SettlementKind::Idle
    );
}

#[test]
fn test_detect_settlement_working_streaming() {
    let log_stream = "Compiling temp-terminal v0.1.0\nDownloading crates...\nRunning cargo build";
    assert_eq!(
        StreamWatcher::detect_settlement(log_stream, "claude"),
        SettlementKind::Working
    );

    let spinner = "Fetching ticket updates... [2/10]";
    assert_eq!(
        StreamWatcher::detect_settlement(spinner, "codex"),
        SettlementKind::Working
    );
}

#[test]
fn test_detect_settlement_exited() {
    let exit_success = "Execution completed.\n[Process exited with code 0]";
    assert_eq!(
        StreamWatcher::detect_settlement(exit_success, "claude"),
        SettlementKind::Exited
    );

    let exit_fail = "Error: fatal error.\nProcess completed with exit code 1";
    assert_eq!(
        StreamWatcher::detect_settlement(exit_fail, "generic"),
        SettlementKind::Exited
    );

    let session_closed = "Disconnected from remote.\nSession closed";
    assert_eq!(
        StreamWatcher::detect_settlement(session_closed, "pi"),
        SettlementKind::Exited
    );
}

#[test]
fn test_format_unprompted_stop_notice() {
    let tail = vec![
        "Line 1: completed task".to_string(),
        "Line 2: ready for next instructions".to_string(),
    ];
    let notice = StreamWatcher::format_unprompted_stop_notice(1, "worker", &tail);
    let expected = "[watch] worker run #1 is idle without reporting. Screen:\nLine 1: completed task\nLine 2: ready for next instructions";
    assert_eq!(notice, expected);

    let empty_tail: Vec<String> = Vec::new();
    let empty_notice = StreamWatcher::format_unprompted_stop_notice(5, "manager", &empty_tail);
    assert_eq!(
        empty_notice,
        "[watch] manager run #5 is idle without reporting. Screen:\n"
    );
}

#[test]
fn test_settlement_kind_serde_roundtrip() {
    let kinds = [
        SettlementKind::Idle,
        SettlementKind::Blocked,
        SettlementKind::Working,
        SettlementKind::Exited,
    ];

    for kind in &kinds {
        let serialized = serde_json::to_string(kind).expect("serialization succeeds");
        let deserialized: SettlementKind =
            serde_json::from_str(&serialized).expect("deserialization succeeds");
        assert_eq!(*kind, deserialized);
    }
}
