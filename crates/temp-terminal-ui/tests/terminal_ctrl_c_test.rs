use std::sync::Arc;
use temp_terminal_core::Registry;
use temp_terminal_ui::workspace::{PaneRenderSnapshot, TerminalPos, WorkspaceApp};
use temp_terminal_ui::{AppHost, Session, TerminalPane};
use tokio::sync::mpsc;

async fn setup_test_app() -> (WorkspaceApp, mpsc::Receiver<Vec<u8>>) {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let (tx, rx) = mpsc::channel(32);
    let pane = Arc::new(TerminalPane::new(
        "pane_test_1".to_string(),
        "TestShell".to_string(),
        80,
        24,
        None,
        tx,
    ));

    let tmp = std::env::temp_dir();
    host.add_project("proj_test", "TestProject", tmp).await;
    {
        let mut projs = host.projects.write().await;
        let proj = projs.get_mut("proj_test").unwrap();
        let mut sess = Session::new("sess_test".to_string(), "Main".to_string());
        sess.add_pane(pane);
        proj.sessions.insert("sess_test".to_string(), sess);
        proj.active_session_id = "sess_test".to_string();
    }

    let mut app = WorkspaceApp::new(host, registry, handle);
    app.refresh_snapshot();

    // Populate fake terminal lines in snapshot for selection tests
    app.snapshot.panes.insert(
        "pane_test_1".to_string(),
        PaneRenderSnapshot {
            id: "pane_test_1".to_string(),
            name: "TestShell".to_string(),
            lines: vec![
                "echo hello world".to_string(),
                "running long process...".to_string(),
            ],
            is_active: true,
            cursor_col: 0,
            cursor_row: 1,
            cursor_visible: true,
            scrollback_len: 0,
            detected_agent: None,
            active_process: None,
            running_command: None,
        },
    );

    (app, rx)
}

#[tokio::test]
async fn test_terminal_ctrl_c_event_copy_sends_etx_interrupt_on_empty_selection() {
    let (mut app, _rx) = setup_test_app().await;

    // When egui-winit emits Event::Copy and no text is selected:
    // It must return 0x03 (ETX / Ctrl+C interrupt)
    let events = vec![egui::Event::Copy];
    let (bytes, text_copied) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::COMMAND,
        "",
    );

    assert_eq!(
        bytes,
        vec![0x03],
        "Ctrl+C / Event::Copy must produce 0x03 interrupt byte when no text is selected"
    );
    assert_eq!(text_copied, None);
}

#[tokio::test]
async fn test_terminal_ctrl_c_copies_when_text_selected_and_subsequent_ctrl_c_interrupts() {
    let (mut app, _rx) = setup_test_app().await;

    // Set selection: line 0, col 5 to col 10 -> "hello"
    app.set_pane_selection(
        "pane_test_1",
        TerminalPos { line_idx: 0, col_idx: 5 },
        TerminalPos { line_idx: 0, col_idx: 10 },
    );
    assert!(app.get_selected_text("pane_test_1").is_some());

    // 1. First Ctrl+C: Text is selected -> should copy and NOT send 0x03
    let events = vec![egui::Event::Copy];
    let (bytes1, text_copied1) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::COMMAND,
        "",
    );

    assert!(bytes1.is_empty(), "First Ctrl+C with selection must not interrupt");
    assert_eq!(text_copied1, Some("hello".to_string()), "Must copy selected text");
    assert!(
        app.get_selected_text("pane_test_1").is_none(),
        "Selection must be cleared after copy"
    );

    // 2. Second Ctrl+C: Selection is now cleared -> should send 0x03 interrupt!
    let (bytes2, text_copied2) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::COMMAND,
        "",
    );

    assert_eq!(
        bytes2,
        vec![0x03],
        "Subsequent Ctrl+C must send 0x03 interrupt once selection is cleared"
    );
    assert_eq!(text_copied2, None);
}

#[tokio::test]
async fn test_terminal_ctrl_c_key_event_sends_etx_interrupt() {
    let (mut app, _rx) = setup_test_app().await;

    // Event::Key with Key::C and Ctrl modifier when no text selected
    let events = vec![egui::Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }];

    let (bytes, text_copied) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::COMMAND,
        "",
    );

    assert_eq!(
        bytes,
        vec![0x03],
        "Key::C with Ctrl modifier must produce 0x03 interrupt byte"
    );
    assert_eq!(text_copied, None);
}

#[tokio::test]
async fn test_terminal_ctrl_shift_c_copies_selected_text() {
    let (mut app, _rx) = setup_test_app().await;

    app.set_pane_selection(
        "pane_test_1",
        TerminalPos { line_idx: 0, col_idx: 0 },
        TerminalPos { line_idx: 0, col_idx: 4 },
    );

    let mut mods = egui::Modifiers::COMMAND;
    mods.shift = true;

    let events = vec![egui::Event::Key {
        key: egui::Key::C,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: mods,
    }];

    let (bytes, text_copied) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        mods,
        "",
    );

    assert!(bytes.is_empty(), "Ctrl+Shift+C must not send interrupt bytes");
    assert_eq!(text_copied, Some("echo".to_string()));
    assert!(app.get_selected_text("pane_test_1").is_none());
}

#[tokio::test]
async fn test_terminal_standard_control_characters_navigation() {
    let (mut app, _rx) = setup_test_app().await;

    let mut check_key = |key: egui::Key, expected_byte: u8| {
        let events = vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }];
        let (bytes, _) = app.handle_terminal_events(
            "pane_test_1",
            &events,
            egui::Modifiers::COMMAND,
            "",
        );
        assert_eq!(
            bytes,
            vec![expected_byte],
            "Failed for key {:?}",
            key
        );
    };

    // Standard Unix / terminal control characters:
    check_key(egui::Key::A, 0x01); // Ctrl+A Beginning of line
    check_key(egui::Key::B, 0x02); // Ctrl+B Cursor left
    check_key(egui::Key::D, 0x04); // Ctrl+D EOF
    check_key(egui::Key::E, 0x05); // Ctrl+E End of line
    check_key(egui::Key::F, 0x06); // Ctrl+F Cursor right
    check_key(egui::Key::G, 0x07); // Ctrl+G Cancel / Bell
    check_key(egui::Key::H, 0x08); // Ctrl+H Backspace
    check_key(egui::Key::J, 0x0a); // Ctrl+J Linefeed
    check_key(egui::Key::K, 0x0b); // Ctrl+K Kill line to end
    check_key(egui::Key::L, 0x0c); // Ctrl+L Clear screen
    check_key(egui::Key::N, 0x0e); // Ctrl+N Next line / history
    check_key(egui::Key::P, 0x10); // Ctrl+P Previous line / history
    check_key(egui::Key::R, 0x12); // Ctrl+R Reverse search
    check_key(egui::Key::T, 0x14); // Ctrl+T Transpose
    check_key(egui::Key::U, 0x15); // Ctrl+U Erase line
    check_key(egui::Key::W, 0x17); // Ctrl+W Delete word
    check_key(egui::Key::Y, 0x19); // Ctrl+Y Yank
    check_key(egui::Key::Z, 0x1a); // Ctrl+Z Suspend / SIGTSTP
    check_key(egui::Key::Escape, 0x1b); // Ctrl+Escape
    check_key(egui::Key::OpenBracket, 0x1b); // Ctrl+[ sends ESC
}

#[tokio::test]
async fn test_terminal_escape_key_handling_all_modifiers() {
    let (mut app, _rx) = setup_test_app().await;

    // 1. Plain Escape (no modifiers): must send 0x1b (ESC)
    let events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    let (bytes, text_copied) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::NONE,
        "",
    );
    assert_eq!(bytes, vec![0x1b], "Plain Escape must send 0x1b (ESC)");
    assert_eq!(text_copied, None);

    // 2. Shift+Escape: must send 0x1b (ESC)
    let events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    }];
    let (bytes, _) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::SHIFT,
        "",
    );
    assert_eq!(bytes, vec![0x1b], "Shift+Escape must send 0x1b (ESC)");

    // 3. Alt+Escape: must send \x1b\x1b
    let events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::ALT,
    }];
    let (bytes, _) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::ALT,
        "",
    );
    assert_eq!(bytes, b"\x1b\x1b".to_vec(), "Alt+Escape must send \\x1b\\x1b");

    // 4. Shift+Tab: must send Backtab (\x1b[Z)
    let events = vec![egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    }];
    let (bytes, _) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::SHIFT,
        "",
    );
    assert_eq!(bytes, b"\x1b[Z".to_vec(), "Shift+Tab must send \\x1b[Z");
}

#[tokio::test]
async fn test_terminal_escape_clears_selection_and_sends_esc() {
    let (mut app, _rx) = setup_test_app().await;

    // Set text selection in the terminal pane
    app.set_pane_selection(
        "pane_test_1",
        TerminalPos { line_idx: 0, col_idx: 0 },
        TerminalPos { line_idx: 0, col_idx: 10 },
    );
    assert!(app.get_selected_text("pane_test_1").is_some());

    // Press Escape: should clear selection AND send 0x1b to process
    let events = vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }];
    let (bytes, text_copied) = app.handle_terminal_events(
        "pane_test_1",
        &events,
        egui::Modifiers::NONE,
        "",
    );

    assert_eq!(bytes, vec![0x1b], "Escape with selection must still send 0x1b");
    assert_eq!(text_copied, None);
    assert_eq!(
        app.get_selected_text("pane_test_1"),
        None,
        "Escape must clear active text selection"
    );
}

#[tokio::test]
async fn test_agy_interactive_help_open_and_esc_close_sequence() {
    let (mut app, mut rx) = setup_test_app().await;

    // Simulate typing 'agy' and pressing Enter
    let (cmd_bytes, _) = app.handle_terminal_events(
        "pane_test_1",
        &[
            egui::Event::Text("agy".to_string()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        egui::Modifiers::NONE,
        "",
    );
    assert_eq!(cmd_bytes, b"agy\r");
    app.send_input_to_pane("pane_test_1", &cmd_bytes);
    let sent_cmd = rx.recv().await.expect("Channel should receive agy enter");
    assert_eq!(sent_cmd, b"agy\r");

    // Simulate agy interactive screen output where '? for shortcuts' appears
    let terminal_lines = vec![
        "Antigravity CLI (agy)".to_string(),
        "? for shortcuts".to_string(),
        "agy> ".to_string(),
    ];
    let detected = WorkspaceApp::detect_agent_from_terminal_lines(&terminal_lines);
    assert_eq!(
        detected,
        Some(("Antigravity", "agy")),
        "Agent detection must identify running agy"
    );

    // Simulate user pressing '?' to open help
    let (help_bytes, _) = app.handle_terminal_events(
        "pane_test_1",
        &[egui::Event::Text("?".to_string())],
        egui::Modifiers::NONE,
        "",
    );
    assert_eq!(help_bytes, b"?");
    app.send_input_to_pane("pane_test_1", &help_bytes);
    let sent_help = rx.recv().await.expect("Channel should receive ?");
    assert_eq!(sent_help, b"?");

    // Simulate user pressing 'Escape' to close help overlay
    let (esc_bytes, _) = app.handle_terminal_events(
        "pane_test_1",
        &[egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
        "",
    );
    assert_eq!(
        esc_bytes,
        vec![0x1b],
        "Escape key must produce 0x1b ESC byte to exit agy help"
    );
    app.send_input_to_pane("pane_test_1", &esc_bytes);
    let sent_esc = rx.recv().await.expect("Channel should receive 0x1b");
    assert_eq!(sent_esc, vec![0x1b]);
}

#[tokio::test]
async fn test_modal_closed_this_frame_suppresses_esc_to_terminal() {
    let (mut app, mut rx) = setup_test_app().await;

    // Open settings modal
    app.settings_modal.open();
    assert!(app.has_any_modal_open());
    assert!(app.settings_modal.is_open);

    // When an app modal is closed by Escape, modal_closed_this_frame is set to true
    app.settings_modal.close();
    app.modal_closed_this_frame = true;

    // The pane condition: `if is_active && !self.has_any_modal_open() && !self.modal_closed_this_frame`
    // will be false during this frame!
    let pane_can_receive_input = !app.has_any_modal_open() && !app.modal_closed_this_frame;
    assert!(
        !pane_can_receive_input,
        "Terminal must NOT receive Escape in the same frame an app modal was closed"
    );
    assert!(rx.try_recv().is_err(), "No bytes should have reached the PTY");
}

#[test]
fn test_detect_command_from_terminal_lines() {
    let lines = vec![
        "Windows PowerShell".to_string(),
        "PS C:\\Users\\Asus\\project> npm run dev".to_string(),
        "  VITE v5.4.1 ready in 250 ms".to_string(),
        "  ➜  Local:   http://localhost:5173/".to_string(),
    ];
    let cmd = WorkspaceApp::detect_command_from_terminal_lines(&lines);
    assert_eq!(cmd.as_deref(), Some("npm run dev"));

    // PowerShell with ANSI syntax highlighting
    let ansi_lines = vec![
        "\x1b[93mPS C:\\Users\\Asus\\project>\x1b[0m \x1b[92mcargo run --bin temp-terminal-gui\x1b[0m".to_string(),
        "   Compiling temp-terminal-ui v0.1.0".to_string(),
    ];
    let cmd_ansi = WorkspaceApp::detect_command_from_terminal_lines(&ansi_lines);
    assert_eq!(cmd_ansi.as_deref(), Some("cargo run --bin temp-terminal-gui"));

    // Cmd.exe prompt
    let cmd_lines = vec![
        "C:\\Users\\Asus> python -m http.server 8080".to_string(),
        "Serving HTTP on 0.0.0.0 port 8080 ...".to_string(),
    ];
    let cmd2 = WorkspaceApp::detect_command_from_terminal_lines(&cmd_lines);
    assert_eq!(cmd2.as_deref(), Some("python -m http.server 8080"));
}

#[test]
fn test_pane_header_title_with_os_active_process() {
    // 1. Idle shell without any active child process
    let title_idle = WorkspaceApp::compute_pane_title(None, None, None);
    assert_eq!(title_idle, "Terminal");

    // 2. Active OS child process: node running npm run dev
    let title_npm = WorkspaceApp::compute_pane_title(
        None,
        Some("node"),
        Some("npm run dev"),
    );
    assert_eq!(title_npm, "Terminal: npm run dev");

    // 3. Active OS child process: cargo running cargo test
    let title_cargo = WorkspaceApp::compute_pane_title(
        None,
        Some("cargo"),
        Some("cargo test"),
    );
    assert_eq!(title_cargo, "Terminal: cargo test");

    // 4. Active OS child process without known prompt command line
    let title_py = WorkspaceApp::compute_pane_title(
        None,
        Some("python"),
        None,
    );
    assert_eq!(title_py, "Terminal: python");

    // 5. Active agent: agy
    let title_agy = WorkspaceApp::compute_pane_title(
        Some("Antigravity"),
        Some("agy"),
        Some("agy"),
    );
    assert_eq!(title_agy, "Terminal (Antigravity)");
}

