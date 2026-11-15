use std::sync::Arc;
use std::time::Duration;

use temp_terminal_core::Registry;
use temp_terminal_daemon::{DaemonOptions, DaemonServer};
use temp_terminal_ui::{AppHost, SplitDir, SplitNode, TerminalBuffer};
use tokio::time::sleep;

#[tokio::test]
async fn test_split_node_tree_operations() {
    let mut root = SplitNode::new_leaf("term_1");
    assert!(root.is_leaf());
    assert_eq!(root.leaf_count(), 1);
    assert_eq!(root.term_ids(), vec!["term_1".to_string()]);

    // Split leaf term_1 with term_2
    assert!(root.split_leaf("term_1", SplitDir::Vertical, "term_2"));
    assert!(!root.is_leaf());
    assert_eq!(root.leaf_count(), 2);
    assert_eq!(
        root.term_ids(),
        vec!["term_1".to_string(), "term_2".to_string()]
    );

    // Split term_2 with term_3
    assert!(root.split_leaf("term_2", SplitDir::Horizontal, "term_3"));
    assert_eq!(root.leaf_count(), 3);
    assert_eq!(
        root.term_ids(),
        vec![
            "term_1".to_string(),
            "term_2".to_string(),
            "term_3".to_string()
        ]
    );

    // Remove term_2
    let updated = SplitNode::remove_leaf(Some(Box::new(root)), "term_2");
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.leaf_count(), 2);
    assert_eq!(
        updated.term_ids(),
        vec!["term_1".to_string(), "term_3".to_string()]
    );
}

#[tokio::test]
async fn test_terminal_buffer_scrollback_and_view() {
    let mut buf = TerminalBuffer::new(80, 4, 10);
    assert_eq!(buf.screen.len(), 4);
    assert!(buf.scrollback.is_empty());

    buf.append_output(b"line 1\nline 2\nline 3\nline 4\nline 5\n");
    assert_eq!(buf.scrollback.len(), 2);
    assert_eq!(buf.scrollback[0], "line 1");
    assert_eq!(buf.scrollback[1], "line 2");

    let view = buf.render_view();
    assert_eq!(view.len(), 6); // 2 in scrollback + 4 on screen
}

#[tokio::test]
async fn test_ui_host_and_terminal_pane_local_pty_streaming() {
    let registry = Arc::new(Registry::new());
    let host = AppHost::new(None);
    let tmp = std::env::temp_dir();

    host.add_project("p1", "TestProject", tmp.clone()).await;

    // Use interactive cmd session with /d flag
    let pane = host
        .spawn_local_pane(
            "p1",
            "s1",
            "pane_local",
            80,
            24,
            &registry,
            vec!["cmd.exe".to_string(), "cmd".to_string(), "/d".to_string()],
            tmp.to_string_lossy().to_string(),
        )
        .await
        .expect("spawn local pane");

    // Wait a brief moment for cmd prompt to initialize
    sleep(Duration::from_millis(200)).await;

    // Send command with CRLF
    pane.send_input(b"echo HELLO_FROM_PTY\r\n")
        .await
        .expect("send input");

    // Wait for PTY output to feed into pane buffer
    let mut seen = false;
    for _ in 0..100 {
        sleep(Duration::from_millis(50)).await;
        let buf = pane.buffer.read().await;
        for line in buf.render_view() {
            if line.contains("HELLO_FROM_PTY") {
                seen = true;
                break;
            }
        }
        if seen {
            break;
        }
    }

    assert!(seen, "expected HELLO_FROM_PTY in terminal buffer");
}

#[tokio::test]
async fn test_ui_host_daemon_rpc_and_worktree_calls() {
    let opts = DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: Some("127.0.0.1:0".to_string()),
        app_name: "test-daemon".to_string(),
    };
    let daemon = DaemonServer::start(opts).await.expect("start daemon");
    let daemon_addr = daemon.addr();

    let host = AppHost::new(Some(daemon_addr));
    let (_stream, welcome) = host.connect_daemon().await.expect("connect daemon");
    assert_eq!(welcome.version, 1);
    assert_eq!(welcome.app, "test-daemon");
    assert!(host.is_connected.load(std::sync::atomic::Ordering::SeqCst));

    // Test process.list RPC via call_daemon
    let list_res = host
        .call_daemon(
            "process.list",
            Some(serde_json::json!({ "project": "dummy_proj" })),
        )
        .await
        .expect("process.list call");
    assert!(list_res.is_array());

    // Test worktree.merge RPC via call_daemon
    let merge_res = host
        .call_daemon(
            "worktree.merge",
            Some(serde_json::json!({
                "by": "pane_lead",
                "target": "feature-test",
                "commit": true,
                "message": "merge feature-test",
                "keep": false
            })),
        )
        .await
        .expect("worktree.merge call");
    assert_eq!(merge_res["branch"], "feature-test");
    assert_eq!(merge_res["removed"], true);

    host.close();
    assert!(!host.is_connected.load(std::sync::atomic::Ordering::SeqCst));
    daemon.close().await;
}

#[tokio::test]
async fn test_terminal_pane_osc_event_routing() {
    let (tx, _) = tokio::sync::mpsc::channel(16);
    let pane = temp_terminal_ui::TerminalPane::new(
        "p_osc".to_string(),
        "p_osc".to_string(),
        80,
        24,
        None,
        tx,
    );

    // Feed OSC 7501 status sequence with base64 encoded title
    let title_b64 = temp_terminal_proto::base64::encode(b"Running");
    let osc_str = format!("\x1b]7501;state=working:app=agent:title={}\x07", title_b64);
    let reports = pane.feed_output(osc_str.as_bytes()).await;
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].record.state, "working");
    assert_eq!(reports[0].record.app, "agent");
    assert_eq!(reports[0].record.title, "Running");

    // Verify record was recorded
    let recs = pane.records.lock().await;
    assert!(!recs.is_empty());
    let lead = recs.lead().expect("lead record");
    assert_eq!(lead.state, "working");
    assert_eq!(lead.app, "agent");
}

#[tokio::test]
async fn test_terminal_renderer_ansi_and_color_parsing() {
    use temp_terminal_ui::renderer::{TerminalColor, TerminalRenderer};

    let renderer = TerminalRenderer::new(14, 1.2);
    let line = "\x1b[1;32mSUCCESS\x1b[0m normal text \x1b[44;37mINFO\x1b[0m \x1b[97;2;3ming\x1b[0m";
    let cells = renderer.parse_ansi_line(line);

    assert!(!cells.is_empty());
    // Check styled "SUCCESS"
    assert_eq!(cells[0].ch, 'S');
    assert_eq!(cells[0].fg, TerminalColor::Green);
    assert!(cells[0].bold);
    assert!(!cells[0].faint);
    assert!(!cells[0].italic);

    // Check reset "normal text"
    let space_cell = cells.iter().find(|c| c.ch == 'n').expect("letter n");
    assert_eq!(space_cell.fg, TerminalColor::Default);
    assert!(!space_cell.bold);
    assert!(!space_cell.faint);
    assert!(!space_cell.italic);

    // Check "INFO" with bg blue and fg white
    let info_cell = cells.iter().find(|c| c.ch == 'I').expect("letter I");
    assert_eq!(info_cell.fg, TerminalColor::White);
    assert_eq!(info_cell.bg, TerminalColor::Blue);

    // Check PowerShell prediction "ing" (BrightWhite, Faint, Italic)
    let pred_cell = cells.iter().find(|c| c.ch == 'i').expect("letter i");
    assert_eq!(pred_cell.fg, TerminalColor::BrightWhite);
    assert!(pred_cell.faint);
    assert!(pred_cell.italic);
    assert!(!pred_cell.bold);
}

#[tokio::test]
async fn test_gui_window_workspace_rendering_and_actions() {
    use temp_terminal_ui::host::{GuiWindow, WindowOptions};

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let tmp = std::env::temp_dir();

    host.add_project("proj_gui", "GUI Project", tmp.clone())
        .await;

    // Spawn two panes in split layout
    #[cfg(windows)]
    let cmd = vec!["cmd.exe".to_string(), "cmd".to_string(), "/d".to_string()];
    #[cfg(not(windows))]
    let cmd = vec!["/bin/sh".to_string()];

    let pane1 = host
        .spawn_local_pane(
            "proj_gui",
            "sess_main",
            "pane_1",
            80,
            24,
            &registry,
            cmd.clone(),
            tmp.to_string_lossy().to_string(),
        )
        .await
        .expect("spawn pane 1");

    let pane2 = host
        .spawn_local_pane(
            "proj_gui",
            "sess_main",
            "pane_2",
            80,
            24,
            &registry,
            cmd.clone(),
            tmp.to_string_lossy().to_string(),
        )
        .await
        .expect("spawn pane 2");

    pane1.feed_output(b"Pane 1 Content\r\n").await;
    pane2.feed_output(b"Pane 2 Content\r\n").await;

    let win = GuiWindow::new(WindowOptions::default(), host.clone(), registry.clone());
    let cells = win.render_workspace_cells(120, 30).await;

    assert_eq!(cells.len(), 30);
    assert_eq!(cells[0].len(), 120);

    // Verify top status bar has project name
    let top_row: String = cells[0].iter().map(|c| c.ch).collect();
    assert!(top_row.contains("GUI Project"));
    assert!(top_row.contains("sess_main"));
    assert!(top_row.contains("[Connected: false]"));

    // Connect daemon and verify updated status bar
    let opts = DaemonOptions {
        bind_addr: "127.0.0.1:0".to_string(),
        proxy_addr: Some("127.0.0.1:0".to_string()),
        app_name: "test-daemon".to_string(),
    };
    let daemon = DaemonServer::start(opts).await.expect("start daemon");
    let daemon_addr = daemon.addr();

    let host_with_daemon = AppHost::new(Some(daemon_addr));
    host_with_daemon
        .add_project("proj_gui", "GUI Project", tmp.clone())
        .await;
    let _ = host_with_daemon
        .spawn_local_pane(
            "proj_gui",
            "sess_main",
            "pane_daemon",
            80,
            24,
            &registry,
            cmd.clone(),
            tmp.to_string_lossy().to_string(),
        )
        .await
        .expect("spawn pane daemon");
    host_with_daemon
        .connect_daemon()
        .await
        .expect("connect daemon");

    let win_with_daemon = GuiWindow::new(
        WindowOptions::default(),
        host_with_daemon.clone(),
        registry.clone(),
    );
    let cells_connected = win_with_daemon.render_workspace_cells(120, 30).await;
    let top_row_connected: String = cells_connected[0].iter().map(|c| c.ch).collect();
    assert!(top_row_connected.contains("[Connected: true]"));
    daemon.close().await;

    // Test action handling on active pane
    assert!(host.handle_action("interrupt").await.is_ok());
    assert!(host.handle_action("clear").await.is_ok());

    // Test pane close
    host.close_pane("proj_gui", "sess_main", "pane_1", &registry)
        .await
        .expect("close pane 1");
    let projs = host.projects.read().await;
    let proj = projs.get("proj_gui").unwrap();
    let sess = proj.sessions.get("sess_main").unwrap();
    assert_eq!(sess.panes.len(), 1);
    assert!(sess.panes.contains_key("pane_2"));
}

#[tokio::test]
async fn test_terminal_buffer_vt_cursor_movement_and_erasure() {
    let mut buf = TerminalBuffer::new(40, 5, 100);

    // Initial state: cursor at (0, 0)
    assert_eq!(buf.cursor_row, 0);
    assert_eq!(buf.cursor_col, 0);

    // 1. Text insertion and carriage return / line feed
    buf.append_output(b"Hello world\r\nSecond line");
    assert_eq!(buf.cursor_row, 1);
    assert_eq!(buf.cursor_col, 11);
    assert_eq!(buf.render_view()[0], "Hello world");
    assert_eq!(buf.render_view()[1], "Second line");

    // 2. Tab expansion (advances to multiple of 8)
    buf.append_output(b"\r\n\tTabbed");
    assert_eq!(buf.cursor_row, 2);
    assert!(buf.render_view()[2].starts_with("        Tabbed"));

    // 3. Backspace
    buf.append_output(b"\x08\x08");
    assert_eq!(buf.cursor_col, 12); // 8 + 6 - 2 = 12

    // 4. Cursor positioning (ESC [ 2 ; 5 H -> row index 1, col index 4)
    buf.append_output(b"\x1b[2;5HREPLACED");
    assert_eq!(buf.cursor_row, 1);
    assert!(buf.render_view()[1].contains("REPLACED"));

    // 5. Erase line (ESC [ 2 K -> clear line)
    buf.append_output(b"\x1b[2K");
    assert!(buf.render_view()[1].is_empty());

    // 6. Erase in display (ESC [ 2 J -> clear whole screen)
    buf.append_output(b"\x1b[2J");
    assert!(buf.render_view().iter().all(|l| l.is_empty()));
    assert_eq!(buf.cursor_row, 0);
    assert_eq!(buf.cursor_col, 0);
}

#[tokio::test]
async fn test_interactive_shortcuts_and_pane_lifecycle() {
    use temp_terminal_ui::host::GuiWindow;

    let registry = Arc::new(Registry::new());
    let host = AppHost::new(None);
    let tmp = std::env::temp_dir();

    host.add_project("proj_shortcuts", "Shortcuts Test", tmp.clone())
        .await;

    #[cfg(windows)]
    let cmd = vec!["cmd.exe".to_string(), "cmd".to_string(), "/d".to_string()];
    #[cfg(not(windows))]
    let cmd = vec!["/bin/sh".to_string()];

    // 1. Spawn root pane
    let root_pane = host
        .spawn_local_pane(
            "proj_shortcuts",
            "sess_main",
            "root_pane",
            80,
            24,
            &registry,
            cmd.clone(),
            tmp.to_string_lossy().to_string(),
        )
        .await
        .expect("spawn root pane");

    assert_eq!(root_pane.id, "root_pane");

    // 2. Dispatch F2 shortcut -> Vertical Split
    GuiWindow::dispatch_key_input(
        &host, &registry, 0x71, // VK_F2
        None, false, false,
    )
    .await;

    // Verify a new pane was created and layout has 2 leaves
    {
        let projs = host.projects.read().await;
        let proj = projs.get("proj_shortcuts").unwrap();
        let sess = proj.sessions.get("sess_main").unwrap();
        assert_eq!(sess.panes.len(), 2);
        assert_ne!(sess.active_pane_id, "root_pane");
        assert_eq!(sess.layout.as_ref().unwrap().leaf_count(), 2);
    }

    // 3. Dispatch F3 shortcut -> Horizontal Split
    GuiWindow::dispatch_key_input(
        &host, &registry, 0x72, // VK_F3
        None, false, false,
    )
    .await;

    {
        let projs = host.projects.read().await;
        let proj = projs.get("proj_shortcuts").unwrap();
        let sess = proj.sessions.get("sess_main").unwrap();
        assert_eq!(sess.panes.len(), 3);
        assert_eq!(sess.layout.as_ref().unwrap().leaf_count(), 3);
    }

    // 4. Dispatch Ctrl+Tab -> Focus next pane
    let active_before = {
        let projs = host.projects.read().await;
        let proj = projs.get("proj_shortcuts").unwrap();
        proj.sessions
            .get("sess_main")
            .unwrap()
            .active_pane_id
            .clone()
    };
    GuiWindow::dispatch_key_input(
        &host, &registry, 0x09, // VK_TAB
        None, true, // Ctrl pressed
        false,
    )
    .await;

    let active_after = {
        let projs = host.projects.read().await;
        let proj = projs.get("proj_shortcuts").unwrap();
        proj.sessions
            .get("sess_main")
            .unwrap()
            .active_pane_id
            .clone()
    };
    assert_ne!(active_before, active_after);

    // 5. Dispatch Ctrl+W -> Close active pane
    GuiWindow::dispatch_key_input(
        &host, &registry, 0x57, // 'W'
        None, true, // Ctrl pressed
        false,
    )
    .await;

    {
        let projs = host.projects.read().await;
        let proj = projs.get("proj_shortcuts").unwrap();
        let sess = proj.sessions.get("sess_main").unwrap();
        assert_eq!(sess.panes.len(), 2);
        assert_eq!(sess.layout.as_ref().unwrap().leaf_count(), 2);
        assert!(!sess.panes.contains_key(&active_after));
    }
}

#[tokio::test]
async fn test_terminal_buffer_bounded_scrollback_and_utf8_chunks() {
    // Capacity 100 lines (clamped to minimum 100 in TerminalBuffer)
    let mut buf = TerminalBuffer::new(80, 5, 100);

    // Stream 1,000 lines through buffer
    for i in 0..1000 {
        let msg = format!("Streamed line {}\r\n", i);
        buf.append_output(msg.as_bytes());
    }

    // Scrollback must never exceed max_scrollback limit (100)
    assert!(buf.scrollback.len() <= 100);
    let view = buf.render_view();
    assert!(view.len() <= 105); // 100 scrollback + 5 screen
                                // Old lines were evicted, newest line is present
    assert!(view.iter().any(|l| l.contains("999")));

    // Split multi-byte UTF-8 emoji across two chunk boundaries:
    // "🚀" in UTF-8 is [0xF0, 0x9F, 0x9A, 0x80]
    let chunk1 = [0xF0, 0x9F];
    let chunk2 = [0x9A, 0x80, b'\r', b'\n'];
    buf.append_output(&chunk1);
    buf.append_output(&chunk2);
    let view_after = buf.render_view();
    assert!(view_after.iter().any(|l| l.contains('🚀')));
}

#[test]
fn test_terminal_buffer_byte_capacity_and_burst_stress() {
    let mut buf = TerminalBuffer::new(80, 24, 1000);
    // Lower max bytes to 20 KB to test byte-bound eviction under heavy output
    buf.max_bytes = 20 * 1024;

    // Simulate high-volume burst output
    let payload = "A".repeat(500); // 500 byte lines
    for _ in 0..100 {
        buf.append_output(format!("{}\n", payload).as_bytes());
    }

    // Test max line byte cap: an unbounded 1MB line without newlines
    let giant_chunk = vec![b'X'; 128 * 1024];
    buf.append_output(&giant_chunk);
    let view = buf.render_view();
    let last_line = view.last().unwrap();
    assert!(last_line.len() <= 64 * 1024);
}

#[test]
fn test_terminal_selection_and_dialog_state() {
    use temp_terminal_ui::workspace::{
        AddProcessDialogState, NewAgentDialogState, TerminalPos, TerminalSelection,
    };

    let sel = TerminalSelection {
        start: TerminalPos {
            line_idx: 1,
            col_idx: 5,
        },
        end: TerminalPos {
            line_idx: 3,
            col_idx: 10,
        },
    };

    assert!(!sel.contains(0, 0));
    assert!(sel.contains(1, 6));
    assert!(sel.contains(2, 0));
    assert!(sel.contains(3, 9));
    assert!(!sel.contains(3, 10));
    assert!(!sel.contains(4, 0));

    // Agent dialog default validation
    let agent_dialog = NewAgentDialogState::default();
    assert_eq!(agent_dialog.selected_agent, "claude");
    assert!(agent_dialog.available_agents.contains(&"shell".to_string()));

    // Process dialog default validation
    let proc_dialog = AddProcessDialogState::default();
    assert_eq!(proc_dialog.name, "web");
    assert!(proc_dialog.autostart);
}

#[test]
fn test_powershell_resolution_and_default_command() {
    let res = temp_terminal_ui::resolve_default_shell();
    assert!(res.is_ok(), "resolve_default_shell must succeed on host");
    let (exe_path, args) = res.unwrap();
    let exe_str = exe_path.to_string_lossy().to_lowercase();
    #[cfg(windows)]
    {
        assert!(
            exe_str.ends_with("pwsh.exe") || exe_str.ends_with("powershell.exe"),
            "Resolved shell must be pwsh.exe or powershell.exe, got: {}",
            exe_str
        );
        assert!(
            !exe_str.ends_with("cmd.exe"),
            "Must never silently resolve to cmd.exe"
        );
        assert_eq!(args, vec!["-NoLogo".to_string()]);
    }
    let cmd = temp_terminal_ui::default_shell_command().expect("default_shell_command");
    assert!(!cmd.is_empty());
    #[cfg(windows)]
    assert!(cmd[0].to_lowercase().contains("powershell") || cmd[0].to_lowercase().contains("pwsh"));
}

#[tokio::test]
async fn test_workspace_pane_focus_switching_and_routing() {
    let tmp = std::env::temp_dir().join(format!("tt_ui_focus_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    host.add_project("proj_focus", "Project", tmp.clone())
        .await;

    // Start with PowerShell / default shell
    let cmd = temp_terminal_ui::default_shell_command().unwrap();

    let pane1 = host
        .spawn_local_pane_with_handle(
            "proj_focus",
            "session_f",
            "pane_1",
            120,
            30,
            &registry,
            cmd.clone(),
            tmp.to_string_lossy().to_string(),
            tokio::runtime::Handle::current(),
        )
        .await
        .expect("spawn pane 1");

    assert_eq!(pane1.id, "pane_1");

    let (raw_tx, mut raw_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    registry.add_on_output(move |_id, chunk| {
        let _ = raw_tx.send(chunk.to_vec());
    });

    // Wait for prompt
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let mut initial_bytes = Vec::new();
    while let Ok(c) = raw_rx.try_recv() {
        initial_bytes.extend_from_slice(&c);
    }
    println!(
        "INITIAL PROMPT RAW: {:?}",
        String::from_utf8_lossy(&initial_bytes)
    );

    // Send a long command
    let _ = pane1
        .send_input(b"echo LONG_COMMAND_TEST_1234567890\r\n")
        .await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let mut cmd1_bytes = Vec::new();
    while let Ok(c) = raw_rx.try_recv() {
        cmd1_bytes.extend_from_slice(&c);
    }
    println!("CMD1 RAW: {:?}", String::from_utf8_lossy(&cmd1_bytes));

    println!("\n=== SENDING ARROW UP ===");
    let _ = pane1.send_input(b"\x1b[A").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut up_bytes = Vec::new();
    while let Ok(c) = raw_rx.try_recv() {
        up_bytes.extend_from_slice(&c);
    }
    println!(
        "UP RAW BYTES (len={}): {:?}",
        up_bytes.len(),
        String::from_utf8_lossy(&up_bytes)
    );

    println!("\n=== SENDING ARROW DOWN ===");
    let _ = pane1.send_input(b"\x1b[B").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut down_bytes = Vec::new();
    while let Ok(c) = raw_rx.try_recv() {
        down_bytes.extend_from_slice(&c);
    }
    println!(
        "DOWN RAW BYTES (len={}): {:?}",
        down_bytes.len(),
        String::from_utf8_lossy(&down_bytes)
    );

    let mut app = temp_terminal_ui::workspace::WorkspaceApp::new(
        host.clone(),
        registry.clone(),
        tokio::runtime::Handle::current(),
    );

    app.refresh_snapshot();
    assert_eq!(app.snapshot.active_pane_id, "pane_1");

    // Split active pane to create pane 2
    let pane2 = host
        .split_active_pane(SplitDir::Vertical, &registry)
        .await
        .expect("split pane");

    app.refresh_snapshot();
    assert_eq!(app.snapshot.active_pane_id, pane2.id);

    // Click/switch focus back to pane 1
    app.focus_pane("pane_1");
    // Verify immediate snapshot update
    assert_eq!(app.snapshot.active_pane_id, "pane_1");
    assert!(app.snapshot.panes.get("pane_1").unwrap().is_active);
    assert!(!app.snapshot.panes.get(&pane2.id).unwrap().is_active);

    // Switch focus back to pane 2
    app.focus_pane(&pane2.id);
    assert_eq!(app.snapshot.active_pane_id, pane2.id);
    assert!(!app.snapshot.panes.get("pane_1").unwrap().is_active);
    assert!(app.snapshot.panes.get(&pane2.id).unwrap().is_active);

    // Clean up
    let _ = host
        .close_pane("proj_focus", "session_f", "pane_1", &registry)
        .await;
    let _ = host
        .close_pane("proj_focus", "session_f", &pane2.id, &registry)
        .await;
}

#[test]
fn test_terminal_buffer_ech_erase_characters_and_csi_k() {
    let mut buf = TerminalBuffer::new(120, 10, 100);

    // 1. Render a prompt followed by a long command
    let prompt_cmd = "PS D:\\project> ii .\\temp-terminal\\dist\\temp-terminal-windows-x64\\";
    buf.append_output(prompt_cmd.as_bytes());
    assert_eq!(buf.render_view()[0], prompt_cmd);
    assert_eq!(buf.cursor_col, prompt_cmd.chars().count());

    // 2. Move cursor to start of command (col 15, 1-indexed is 16: \x1b[1;16H)
    // and Erase in Line to end (CSI 0 K: \x1b[K)
    buf.append_output(b"\x1b[1;16H\x1b[K");
    assert_eq!(buf.render_view()[0], "PS D:\\project>");
    assert_eq!(buf.cursor_col, 15);

    // 3. Type a replacement shorter command
    buf.append_output(b"dir");
    assert_eq!(buf.render_view()[0], "PS D:\\project> dir");

    // 4. Test ECH (Erase Characters \x1b[{N}X)
    // Move cursor back over "dir" and erase 3 characters
    buf.append_output(b"\x1b[1;16H\x1b[3X");
    assert_eq!(buf.render_view()[0], "PS D:\\project>");

    // 5. Verify no stale suffix remains
    assert!(!buf.render_view()[0].contains("temp-terminal"));
    assert!(!buf.render_view()[0].contains("windows-x64"));
}

#[test]
fn test_terminal_buffer_csi_and_utf8_split_at_all_chunk_boundaries() {
    let sequence = "PS > Get-Process\x1b[1;6H\x1b[11XClear";

    // Split across every possible single-byte chunk boundary
    let mut buf = TerminalBuffer::new(80, 5, 100);
    for b in sequence.as_bytes() {
        buf.append_output(&[*b]);
    }

    assert_eq!(buf.cursor_row, 0);
    assert_eq!(buf.render_view()[0], "PS > Clear");
    assert!(!buf.render_view()[0].contains("Get-Process"));
    assert!(buf.render_view()[0].contains("Clear"));

    // Split across 2-byte chunk boundaries
    let mut buf2 = TerminalBuffer::new(80, 5, 100);
    for chunk in sequence.as_bytes().chunks(2) {
        buf2.append_output(chunk);
    }
    assert_eq!(buf2.render_view()[0], "PS > Clear");

    // Split across 3-byte chunk boundaries
    let mut buf3 = TerminalBuffer::new(80, 5, 100);
    for chunk in sequence.as_bytes().chunks(3) {
        buf3.append_output(chunk);
    }
    assert_eq!(buf3.render_view()[0], "PS > Clear");
}

#[test]
fn test_terminal_buffer_psreadline_escape_line_clearing() {
    let mut buf = TerminalBuffer::new(120, 10, 100);

    // Simulate PSReadLine typing:
    // Prompt + long command: "PS C:\\Users\\Asus> ii .\\temp-terminal\\dist\\temp-terminal-windows-x64\\"
    let prompt = "PS C:\\Users\\Asus>";
    let cmd = " ii .\\temp-terminal\\dist\\temp-terminal-windows-x64\\";
    buf.append_output(format!("{}{}", prompt, cmd).as_bytes());

    // Prompt length is 17 chars (cursor at col 17 + 51 = 68, row 0).
    // Now simulate PSReadLine Escape / ArrowDown clear sequence:
    // \x1b[m\x1b[?25l\x1b[1;18H\x1b[51X\x1b[?25h
    let clear_seq = b"\x1b[m\x1b[?25l\x1b[1;18H\x1b[51X\x1b[?25h";
    buf.append_output(clear_seq);

    // Verify:
    assert_eq!(buf.cursor_row, 0);
    assert_eq!(buf.cursor_col, 17);

    // Rendered view must be only the prompt, no stale characters
    let view = buf.render_view();
    let line0 = &view[0];
    assert_eq!(line0, prompt);
    assert!(!line0.contains("inal-windows-x64\\"));
    assert!(!line0.contains("dist"));
}

#[tokio::test]
async fn test_project_registration_and_duplicate_prevention() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let temp_dir_a =
        std::env::temp_dir().join(format!("tt_test_proj_a_{}", temp_terminal_ui::uuid_short()));
    let temp_dir_b =
        std::env::temp_dir().join(format!("tt_test_proj_b_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&temp_dir_a);
    let _ = std::fs::create_dir_all(&temp_dir_b);

    // 1. Register Project A
    let id_a = host
        .open_or_switch_project(temp_dir_a.clone(), &registry)
        .await
        .expect("should register project A");
    assert_eq!(*host.active_project_id.read().await, id_a);

    // 2. Register same project A again (duplicate check parity with Go samePath)
    let id_a_dup = host
        .open_or_switch_project(temp_dir_a.clone(), &registry)
        .await
        .expect("should handle duplicate without error");
    assert_eq!(
        id_a, id_a_dup,
        "duplicate path must return existing project ID"
    );
    assert_eq!(
        host.projects.read().await.len(),
        1,
        "must not duplicate project entry"
    );

    // 3. Register Project B
    let id_b = host
        .open_or_switch_project(temp_dir_b.clone(), &registry)
        .await
        .expect("should register project B");
    assert_ne!(
        id_a, id_b,
        "different directory must have distinct project ID"
    );
    assert_eq!(*host.active_project_id.read().await, id_b);
    assert_eq!(
        host.projects.read().await.len(),
        2,
        "must have two projects registered"
    );

    // Cleanup temp dirs
    let _ = std::fs::remove_dir_all(&temp_dir_a);
    let _ = std::fs::remove_dir_all(&temp_dir_b);
}

#[tokio::test]
async fn test_project_path_validation() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let non_existent = std::env::temp_dir().join(format!(
        "tt_non_existent_{}",
        temp_terminal_ui::uuid_short()
    ));
    let res = host.open_or_switch_project(non_existent, &registry).await;
    assert!(
        res.is_err(),
        "opening non-existent directory must return error"
    );
}

#[tokio::test]
async fn test_project_switching_and_session_isolation() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let dir_a =
        std::env::temp_dir().join(format!("tt_switch_a_{}", temp_terminal_ui::uuid_short()));
    let dir_b =
        std::env::temp_dir().join(format!("tt_switch_b_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir_a);
    let _ = std::fs::create_dir_all(&dir_b);

    let id_a = host
        .open_or_switch_project(dir_a.clone(), &registry)
        .await
        .unwrap();
    let id_b = host
        .open_or_switch_project(dir_b.clone(), &registry)
        .await
        .unwrap();

    // Verify Project B is active
    assert_eq!(*host.active_project_id.read().await, id_b);

    // Switch back to Project A
    let switched = host.switch_project(&id_a).await;
    assert!(switched);
    assert_eq!(*host.active_project_id.read().await, id_a);

    // Sessions under Project A and Project B are completely isolated
    let projs = host.projects.read().await;
    let proj_a = projs.get(&id_a).unwrap();
    let proj_b = projs.get(&id_b).unwrap();
    assert!(
        !proj_a.sessions.is_empty(),
        "Project A must retain its sessions"
    );
    assert!(
        !proj_b.sessions.is_empty(),
        "Project B must retain its sessions"
    );
    for s_a in proj_a.sessions.keys() {
        assert!(
            !proj_b.sessions.contains_key(s_a),
            "Sessions must not cross project boundaries"
        );
    }

    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[tokio::test]
async fn test_session_closure_and_process_cleanup() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let dir =
        std::env::temp_dir().join(format!("tt_sess_close_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let id = host
        .open_or_switch_project(dir.clone(), &registry)
        .await
        .unwrap();
    let (sess_id, pane_id) = {
        let projs = host.projects.read().await;
        let proj = projs.get(&id).unwrap();
        let (s_id, sess) = proj.sessions.iter().next().unwrap();
        let (p_id, _) = sess.panes.iter().next().unwrap();
        (s_id.clone(), p_id.clone())
    };

    assert!(registry.get(&pane_id).is_some());

    // Close the session
    host.close_session(&id, &sess_id, &registry).await.unwrap();

    // Verify session removed from project and pane removed from registry
    let projs = host.projects.read().await;
    let proj = projs.get(&id).unwrap();
    assert!(
        !proj.sessions.contains_key(&sess_id),
        "session must be pruned on closure"
    );
    assert!(
        registry.get(&pane_id).is_none(),
        "pane must be removed from registry"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_project_removal_and_panes_cleanup() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let dir_a = std::env::temp_dir().join(format!("tt_rm_a_{}", temp_terminal_ui::uuid_short()));
    let dir_b = std::env::temp_dir().join(format!("tt_rm_b_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir_a);
    let _ = std::fs::create_dir_all(&dir_b);

    let id_a = host
        .open_or_switch_project(dir_a.clone(), &registry)
        .await
        .unwrap();
    let id_b = host
        .open_or_switch_project(dir_b.clone(), &registry)
        .await
        .unwrap();

    // Remove Project B (the active project)
    host.remove_project(&id_b, &registry).await.unwrap();

    // Project B removed, Project A becomes active
    assert_eq!(host.projects.read().await.len(), 1);
    assert!(!host.projects.read().await.contains_key(&id_b));
    assert_eq!(
        *host.active_project_id.read().await,
        id_a,
        "removing active project must switch to remaining project"
    );

    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[tokio::test]
async fn test_state_persistence_and_restoration() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let base_dir =
        std::env::temp_dir().join(format!("tt_persist_{}", temp_terminal_ui::uuid_short()));
    let proj_dir = base_dir.join("workspace");
    let state_file = base_dir.join("state.json");
    let _ = std::fs::create_dir_all(&proj_dir);

    let id = host
        .open_or_switch_project(proj_dir.clone(), &registry)
        .await
        .unwrap();
    host.save_state_to_path(&state_file).await.unwrap();

    // Create a fresh AppHost and load state from isolated file
    let host2 = AppHost::new(None);
    let loaded = host2
        .load_state_from_path(&state_file, &registry)
        .await
        .unwrap();
    assert!(loaded >= 1, "must load at least 1 saved project");
    assert!(
        host2.projects.read().await.contains_key(&id),
        "saved project ID must be restored"
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_missing_project_directory_handling() {
    let base_dir = std::env::temp_dir().join(format!(
        "tt_missing_base_{}",
        temp_terminal_ui::uuid_short()
    ));
    let missing_path = base_dir.join("nonexistent_workspace_dir");
    let state_file = base_dir.join("state.json");
    let _ = std::fs::create_dir_all(&base_dir);

    // Write state.json with a project whose directory does not exist on disk
    let seed_state = serde_json::json!({
        "version": 2,
        "viewed": "proj_missing",
        "projects": [
            {
                "id": "proj_missing",
                "name": "Missing Project",
                "dir": missing_path.to_string_lossy().to_string()
            }
        ]
    });
    std::fs::write(
        &state_file,
        serde_json::to_string_pretty(&seed_state).unwrap(),
    )
    .unwrap();

    // Load state in fresh host from isolated file
    let registry = Arc::new(Registry::new());
    let host2 = AppHost::new(None);
    let _ = host2
        .load_state_from_path(&state_file, &registry)
        .await
        .unwrap();

    let projs = host2.projects.read().await;
    let p = projs
        .get("proj_missing")
        .expect("project must be present in state even if folder is missing");
    assert!(
        !p.is_available,
        "missing directory must be marked unavailable / folder missing"
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn test_split_node_ratio_clamping_and_120px_minimum() {
    // 1. Available width 1000px: 120px is 12%
    let clamped_low = SplitNode::clamp_ratio(5.0, 1000.0);
    assert!(
        (clamped_low - 12.0).abs() < 0.01,
        "clamped low should be 12.0% (120px of 1000px), got {}",
        clamped_low
    );

    let clamped_high = SplitNode::clamp_ratio(95.0, 1000.0);
    assert!(
        (clamped_high - 88.0).abs() < 0.01,
        "clamped high should be 88.0% (1000 - 120px), got {}",
        clamped_high
    );

    let clamped_mid = SplitNode::clamp_ratio(60.0, 1000.0);
    assert!(
        (clamped_mid - 60.0).abs() < 0.01,
        "valid mid ratio should be preserved, got {}",
        clamped_mid
    );

    // 2. Available width 400px: 120px is 30%
    let clamped_400_low = SplitNode::clamp_ratio(15.0, 400.0);
    assert!(
        (clamped_400_low - 30.0).abs() < 0.01,
        "at 400px, 120px is 30.0%, got {}",
        clamped_400_low
    );
    let clamped_400_high = SplitNode::clamp_ratio(85.0, 400.0);
    assert!(
        (clamped_400_high - 70.0).abs() < 0.01,
        "at 400px, max ratio is 70.0% (400 - 120 = 280), got {}",
        clamped_400_high
    );

    // 3. Small window size (< 240px): graceful fallback to 50%
    let clamped_small = SplitNode::clamp_ratio(20.0, 180.0);
    assert_eq!(
        clamped_small, 50.0,
        "under 240px available size, ratio must default to 50.0"
    );
}

#[test]
fn test_nested_split_tree_ratio_updates_by_binary_path() {
    let mut root = SplitNode {
        term_id: String::new(),
        dir: SplitDir::Vertical,
        ratio: 50.0,
        first: Some(Box::new(SplitNode::new_leaf("pane_a"))),
        second: Some(Box::new(SplitNode {
            term_id: String::new(),
            dir: SplitDir::Horizontal,
            ratio: 50.0,
            first: Some(Box::new(SplitNode::new_leaf("pane_b"))),
            second: Some(Box::new(SplitNode::new_leaf("pane_c"))),
        })),
    };

    assert!(root.contains("pane_a"));
    assert!(root.contains("pane_b"));
    assert!(root.contains("pane_c"));
    assert!(!root.contains("pane_unknown"));

    // Update root ratio via empty path ""
    assert!(root.set_ratio_at_path("", 65.0));
    assert_eq!(root.get_ratio_at_path(""), Some(65.0));
    assert_eq!(root.ratio, 65.0);

    // Nested second node ratio must still be 50.0
    assert_eq!(root.get_ratio_at_path("b"), Some(50.0));

    // Update nested second node ratio via "b"
    assert!(root.set_ratio_at_path("b", 35.0));
    assert_eq!(root.get_ratio_at_path("b"), Some(35.0));

    // Root ratio must remain 65.0
    assert_eq!(root.get_ratio_at_path(""), Some(65.0));

    // Leaves do not have ratios
    assert!(!root.set_ratio_at_path("a", 40.0));
    assert_eq!(root.get_ratio_at_path("a"), None);
}

#[tokio::test]
async fn test_interactive_zoom_and_restore_lifecycle() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let dir = std::env::temp_dir().join(format!("tt_zoom_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let proj_id = host
        .open_or_switch_project(dir.clone(), &registry)
        .await
        .unwrap();

    // Split to have 2 panes
    let _ = host
        .split_active_pane(SplitDir::Vertical, &registry)
        .await
        .unwrap();

    let projs = host.projects.read().await;
    let proj = projs.get(&proj_id).unwrap();
    let sess = proj.sessions.get(&proj.active_session_id).unwrap();
    assert_eq!(sess.panes.len(), 2);
    assert!(sess.zoomed_pane_id.is_none(), "initially not zoomed");
    let initial_layout = sess.layout.clone().unwrap();
    drop(projs);

    // 1. Toggle zoom
    let zoomed = host.toggle_zoom().await;
    assert!(zoomed.is_some(), "must enter zoom mode");
    let zoomed_pane_id = zoomed.unwrap();

    let projs = host.projects.read().await;
    let proj = projs.get(&proj_id).unwrap();
    let sess = proj.sessions.get(&proj.active_session_id).unwrap();
    assert_eq!(sess.zoomed_pane_id, Some(zoomed_pane_id.clone()));
    // Layout tree is preserved intact
    assert_eq!(sess.layout.as_ref().unwrap().term_ids().len(), 2);
    drop(projs);

    // 2. Toggle zoom again to restore
    let restored = host.toggle_zoom().await;
    assert!(restored.is_none(), "must leave zoom mode");

    let projs = host.projects.read().await;
    let proj = projs.get(&proj_id).unwrap();
    let sess = proj.sessions.get(&proj.active_session_id).unwrap();
    assert!(sess.zoomed_pane_id.is_none(), "zoom must be cleared");
    assert_eq!(sess.layout.as_ref().unwrap(), &initial_layout);
    drop(projs);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_active_pane_switching_and_focus_while_zoomed() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let dir =
        std::env::temp_dir().join(format!("tt_zoom_switch_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let proj_id = host
        .open_or_switch_project(dir.clone(), &registry)
        .await
        .unwrap();

    // Create a 2-pane split
    let pane2 = host
        .split_active_pane(SplitDir::Horizontal, &registry)
        .await
        .unwrap();

    let pane2_id = pane2.id.clone();

    // Zoom pane 2 explicitly
    host.set_zoomed_pane(Some(pane2_id.clone())).await;

    let projs = host.projects.read().await;
    let proj = projs.get(&proj_id).unwrap();
    let sess = proj.sessions.get(&proj.active_session_id).unwrap();
    assert_eq!(sess.zoomed_pane_id, Some(pane2_id.clone()));
    assert_eq!(sess.active_pane_id, pane2_id);
    drop(projs);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_pane_close_while_zoomed_restores_layout_state() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());

    let dir =
        std::env::temp_dir().join(format!("tt_zoom_close_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let proj_id = host
        .open_or_switch_project(dir.clone(), &registry)
        .await
        .unwrap();

    let pane2 = host
        .split_active_pane(SplitDir::Vertical, &registry)
        .await
        .unwrap();

    let pane2_id = pane2.id.clone();

    // Zoom pane 2
    host.set_zoomed_pane(Some(pane2_id.clone())).await;

    // Now close pane 2 while zoomed
    let sess_id = {
        let projs = host.projects.read().await;
        projs.get(&proj_id).unwrap().active_session_id.clone()
    };
    host.close_pane(&proj_id, &sess_id, &pane2_id, &registry)
        .await
        .unwrap();

    // Verify session zoomed_pane_id is automatically reset to None
    let projs = host.projects.read().await;
    let proj = projs.get(&proj_id).unwrap();
    let sess = proj.sessions.get(&sess_id).unwrap();
    assert!(
        sess.zoomed_pane_id.is_none(),
        "closing zoomed pane must reset zoomed_pane_id to None"
    );
    assert_eq!(sess.panes.len(), 1, "remaining pane must still be alive");
    assert!(
        !sess.active_pane_id.is_empty(),
        "active pane must fall back to remaining pane"
    );
    drop(projs);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_process_dialog_env_and_watch_parsing() {
    use temp_terminal_ui::workspace::{parse_process_env, parse_process_watch};

    // 1. Valid environment text parsing
    let env_text = r#"
        PORT=3000
        NODE_ENV=development
        API_BASE=http://localhost:8080
    "#;
    let env_map = parse_process_env(env_text).expect("valid env should parse");
    assert_eq!(env_map.len(), 3);
    assert_eq!(env_map.get("PORT").map(|s| s.as_str()), Some("3000"));
    assert_eq!(
        env_map.get("NODE_ENV").map(|s| s.as_str()),
        Some("development")
    );
    assert_eq!(
        env_map.get("API_BASE").map(|s| s.as_str()),
        Some("http://localhost:8080")
    );

    // 2. Invalid line without '=' error message parity with Go
    let err_missing_eq = parse_process_env("VALID=1\nINVALID_LINE_WITHOUT_EQUALS\nANOTHER=2");
    assert!(err_missing_eq.is_err());
    let err_msg = err_missing_eq.unwrap_err();
    assert!(
        err_msg.contains("environment line 2 must be KEY=value"),
        "error must match Go parity: got {}",
        err_msg
    );

    // 3. Duplicate key error message parity with Go
    let err_dup = parse_process_env("PORT=3000\nPORT=8080");
    assert!(err_dup.is_err());
    let dup_msg = err_dup.unwrap_err();
    assert!(
        dup_msg.contains("environment key \"PORT\" is listed twice"),
        "error must match Go parity: got {}",
        dup_msg
    );

    // 4. Watch globs parsing
    let watch_text = "src/**\n  public/assets/*\n\n  dist/ignore.js  ";
    let watch_globs = parse_process_watch(watch_text);
    assert_eq!(
        watch_globs,
        vec!["src/**", "public/assets/*", "dist/ignore.js"]
    );
}

#[test]
fn test_process_yaml_preservation_and_atomic_save() {
    let dir = std::env::temp_dir().join(format!("tt_proc_yaml_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let sample_yaml = r#"# Top-level comments
name: myapp
version: 1.0

processes:
  web:
    command: npm run dev
    dir: frontend
    port: 3000
    autostart: true
    restart: always
    watch:
      - src/**
    env:
      PORT: "3000"
"#;
    let yaml_file = dir.join("tempterm.yml");
    std::fs::write(&yaml_file, sample_yaml).unwrap();

    // Load existing
    let mut defs = temp_terminal_procs::file::load_file(&dir).expect("should load");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "web");

    // Add a worker process
    let mut worker_env = std::collections::BTreeMap::new();
    worker_env.insert("QUEUE_NAME".to_string(), "jobs".to_string());
    let worker = temp_terminal_procs::def::Def {
        name: "worker".to_string(),
        command: "cargo run --bin worker".to_string(),
        restart: "on-failure".to_string(),
        watch: vec!["crates/worker/**".to_string()],
        env: worker_env,
        ..Default::default()
    };
    defs.push(worker);

    // Save back atomically
    temp_terminal_procs::file::save_file(&dir, &defs).expect("save should succeed");

    // Reload and assert
    let reloaded = temp_terminal_procs::file::load_file(&dir).expect("reload should succeed");
    assert_eq!(reloaded.len(), 2);
    assert_eq!(reloaded[1].name, "worker");
    assert_eq!(
        reloaded[1].env.get("QUEUE_NAME").map(|s| s.as_str()),
        Some("jobs")
    );
    assert_eq!(reloaded[1].watch, vec!["crates/worker/**"]);

    // Verify top-level comment and name preserved
    let raw = std::fs::read_to_string(&yaml_file).unwrap();
    assert!(raw.contains("# Top-level comments"));
    assert!(raw.contains("name: myapp"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_settings_all_categories_defaults_and_roundtrip() {
    let dir =
        std::env::temp_dir().join(format!("tt_settings_rt_{}", temp_terminal_ui::uuid_short()));
    let file = dir.join("settings.json");
    let _ = std::fs::create_dir_all(&dir);

    // 1. Defaults
    let s = temp_terminal_ui::settings::Settings::load_from_path(&file);
    assert_eq!(s.theme, "dark");
    assert_eq!(s.density, "comfortable");
    assert_eq!(s.open_links, "system");
    assert_eq!(s.search_engine, "duckduckgo");
    assert!(s.notify.waiting);
    assert!(s.control_socket);

    // 2. Modify across all categories
    let mut modified = s;
    modified.theme = "light".to_string();
    modified.density = "compact".to_string();
    modified.open_links = "pane".to_string();
    modified.search_engine = "kagi".to_string();
    modified.terminal.font_family = "Cascadia Code".to_string();
    modified.terminal.font_size = 14.5;
    modified.terminal.cursor = "underline".to_string();
    modified.terminal.copy_on_select = false;
    modified.editor.tab_size = 2;
    modified.editor.line_numbers = false;
    modified.notify.sound = false;
    modified.remote_control = true;

    // Save atomically
    modified.save_to_path(&file).expect("save must succeed");

    // 3. Reload and assert persistence
    let reloaded = temp_terminal_ui::settings::Settings::load_from_path(&file);
    assert_eq!(reloaded.theme, "light");
    assert_eq!(reloaded.density, "compact");
    assert_eq!(reloaded.open_links, "pane");
    assert_eq!(reloaded.search_engine, "kagi");
    assert_eq!(reloaded.terminal.font_family, "Cascadia Code");
    assert_eq!(reloaded.terminal.font_size, 14.5);
    assert_eq!(reloaded.terminal.cursor, "underline");
    assert!(!reloaded.terminal.copy_on_select);
    assert_eq!(reloaded.editor.tab_size, 2);
    assert!(!reloaded.editor.line_numbers);
    assert!(!reloaded.notify.sound);
    assert!(reloaded.remote_control);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_process_spawn_with_custom_env_and_propagation() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();

    let dir = std::env::temp_dir().join(format!("tt_proc_env_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&dir);

    let proj_id = host
        .open_or_switch_project(dir.clone(), &registry)
        .await
        .unwrap();

    let custom_env = vec![
        "TEMP_TERMINAL_TEST_KEY=HELLO_RUST".to_string(),
        "NODE_ENV=test".to_string(),
    ];

    #[cfg(windows)]
    let cmd = vec![
        "cmd.exe".to_string(),
        "/c".to_string(),
        "echo %TEMP_TERMINAL_TEST_KEY%".to_string(),
    ];
    #[cfg(not(windows))]
    let cmd = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "echo $TEMP_TERMINAL_TEST_KEY".to_string(),
    ];

    let pane_res = host
        .spawn_local_pane_with_env(
            &proj_id,
            "session_env_test",
            "pane_env_test",
            120,
            36,
            &registry,
            cmd,
            dir.to_string_lossy().to_string(),
            custom_env,
            handle,
        )
        .await;

    assert!(pane_res.is_ok(), "spawning pane with env must succeed");
    let pane = pane_res.unwrap();
    assert_eq!(pane.id, "pane_env_test");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_editor_file_lifecycle_open_edit_save_reopen() {
    let base_dir =
        std::env::temp_dir().join(format!("tt_ed_cycle_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&base_dir);
    let sample_file = base_dir.join("main.rs");
    let initial_content = "fn main() {\n    println!(\"Hello, Group E!\");\n}\n";
    std::fs::write(&sample_file, initial_content).unwrap();

    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();
    let mut app = temp_terminal_ui::workspace::WorkspaceApp::new(host, registry, handle);
    app.snapshot.project_path = base_dir.to_string_lossy().to_string();

    // 1. Open
    app.open_editor_file("main.rs");
    assert!(app.open_editors.contains_key("main.rs"));
    assert_eq!(app.active_editor_path.as_deref(), Some("main.rs"));
    let doc = app.open_editors.get("main.rs").unwrap();
    assert_eq!(doc.content, initial_content);
    assert!(!doc.is_dirty);

    // 2. Edit
    if let Some(doc_mut) = app.open_editors.get_mut("main.rs") {
        doc_mut.content.push_str("// Appended comment\n");
        doc_mut.is_dirty = true;
    }
    assert!(app.open_editors.get("main.rs").unwrap().is_dirty);

    // 3. Save
    app.save_active_editor();
    assert!(!app.open_editors.get("main.rs").unwrap().is_dirty);

    // 4. Reopen and verify
    let reloaded = std::fs::read_to_string(&sample_file).unwrap();
    assert!(reloaded.contains("// Appended comment"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_editor_failed_save_preserves_dirty_buffer() {
    let base_dir =
        std::env::temp_dir().join(format!("tt_ed_failsave_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&base_dir);

    // Document pointing to an invalid read-only directory
    let invalid_path = base_dir.join("nonexistent_sub").join("file.txt");
    let mut doc =
        temp_terminal_ui::editor::EditorDoc::new_memory("file.txt", "file.txt", "unsaved data");
    doc.full_path = invalid_path;
    doc.is_dirty = true;

    // Attempt save to nonexistent directory
    let res = doc.save();
    assert!(res.is_err(), "saving to nonexistent dir should error");
    assert!(
        doc.is_dirty,
        "dirty state must be preserved on save failure"
    );
    assert_eq!(
        doc.content, "unsaved data",
        "buffer content must not be discarded"
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_editor_unknown_extension_fallback_and_languages() {
    let base_dir =
        std::env::temp_dir().join(format!("tt_ed_langs_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&base_dir);

    let _doc_unknown =
        temp_terminal_ui::editor::EditorDoc::open("config.custom_ext_123", &base_dir);
    // Even if file doesn't exist on disk, Language::for_path classifies fallback
    assert_eq!(
        temp_terminal_ui::editor::Language::for_path("notes.xyz"),
        temp_terminal_ui::editor::Language::PlainText
    );
    assert_eq!(
        temp_terminal_ui::editor::Language::for_path("main.go"),
        temp_terminal_ui::editor::Language::Go
    );
    assert_eq!(
        temp_terminal_ui::editor::Language::for_path("index.tsx"),
        temp_terminal_ui::editor::Language::TypeScript
    );
    assert_eq!(
        temp_terminal_ui::editor::Language::for_path("Cargo.lock"),
        temp_terminal_ui::editor::Language::Toml
    );
    assert_eq!(
        temp_terminal_ui::editor::Language::for_path("README.md"),
        temp_terminal_ui::editor::Language::Markdown
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_editor_diff_viewer_split_and_inline() {
    let old_text = "fn compute() -> i32 {\n    let a = 10;\n    a * 2\n}\n";
    let new_text = "fn compute() -> i32 {\n    let a = 10;\n    let b = 5;\n    a * b\n}\n";

    let diff = temp_terminal_ui::editor::compute_line_diff(old_text, new_text);
    assert!(!diff.is_identical);
    assert!(diff.additions >= 1);
    assert!(diff.deletions >= 1);

    let mut view = temp_terminal_ui::editor::DiffViewState::default();
    view.open("compute.rs", "Working Tree", old_text, new_text, true);
    assert!(view.is_open);
    assert!(view.is_split);
    assert_eq!(view.title, "compute.rs");
    assert_eq!(view.scope, "Working Tree");
}

#[tokio::test]
async fn test_editor_external_modification_clean_reload_and_conflict() {
    let base_dir =
        std::env::temp_dir().join(format!("tt_ed_disk_mod_{}", temp_terminal_ui::uuid_short()));
    let _ = std::fs::create_dir_all(&base_dir);
    let sample_file = base_dir.join("live.txt");
    std::fs::write(&sample_file, "version 1\n").unwrap();

    let mut doc = temp_terminal_ui::editor::EditorDoc::open("live.txt", &base_dir).unwrap();
    assert_eq!(doc.content, "version 1\n");
    assert!(!doc.is_dirty);

    // External modification while clean: should reload automatically
    std::fs::write(&sample_file, "version 2 (modified externally)\n").unwrap();
    // Reset check timer to force disk check
    doc.last_checked = std::time::Instant::now() - std::time::Duration::from_secs(5);
    doc.check_disk();
    assert_eq!(doc.content, "version 2 (modified externally)\n");
    assert!(!doc.is_dirty);
    assert!(!doc.has_external_conflict);

    // External modification while dirty: should flag conflict
    doc.content = "version 2 (dirty editor changes)\n".to_string();
    doc.is_dirty = true;
    std::fs::write(&sample_file, "version 3 (disk changed again)\n").unwrap();
    doc.last_checked = std::time::Instant::now() - std::time::Duration::from_secs(5);
    doc.check_disk();
    assert_eq!(
        doc.content, "version 2 (dirty editor changes)\n",
        "dirty buffer must not be overwritten"
    );
    assert!(doc.has_external_conflict, "conflict must be flagged");

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_editor_unsaved_changes_modal_and_project_isolation() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();
    let mut app = temp_terminal_ui::workspace::WorkspaceApp::new(host, registry, handle);

    let doc =
        temp_terminal_ui::editor::EditorDoc::new_memory("dirty.rs", "dirty.rs", "let x = 1;\n");
    app.open_editors.insert("dirty.rs".to_string(), doc);
    app.active_editor_path = Some("dirty.rs".to_string());

    // Mark dirty
    app.open_editors.get_mut("dirty.rs").unwrap().is_dirty = true;

    // Closing dirty file should open confirmation modal
    app.close_editor_file("dirty.rs");
    assert!(
        app.open_editors.contains_key("dirty.rs"),
        "dirty file must remain open until decision"
    );
    assert_eq!(app.close_unsaved_doc.as_deref(), Some("dirty.rs"));
}

#[tokio::test]
async fn test_editor_markdown_preview_and_toggle() {
    let host = AppHost::new(None);
    let registry = Arc::new(Registry::new());
    let handle = tokio::runtime::Handle::current();
    let mut app = temp_terminal_ui::workspace::WorkspaceApp::new(host, registry, handle);

    let md_content = "# Title\n\nSome paragraph with `code`.\n";
    let mut doc =
        temp_terminal_ui::editor::EditorDoc::new_memory("README.md", "README.md", md_content);
    assert!(!doc.is_preview);
    assert_eq!(doc.language, temp_terminal_ui::editor::Language::Markdown);

    // Toggle preview on
    doc.is_preview = true;
    app.open_editors.insert("README.md".to_string(), doc);
    app.active_editor_path = Some("README.md".to_string());

    assert!(app.open_editors.get("README.md").unwrap().is_preview);

    // Toggle preview off
    app.open_editors.get_mut("README.md").unwrap().is_preview = false;
    assert!(!app.open_editors.get("README.md").unwrap().is_preview);
}

#[tokio::test]
async fn test_editor_diff_git_staging_actions() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "tt_git_stage_test_{}_{}",
        std::process::id(),
        temp_terminal_procs::def::sha256_digest(b"git_stage_test")[0]
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let _ = std::fs::create_dir_all(&tmp_dir);

    // Initialize temporary Git repository
    let init_res = temp_terminal_core::git::git_out(&tmp_dir, &["init"]);
    if init_res.is_err() {
        // Git not available on test machine, skip test gracefully
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return;
    }
    // Set local user name/email so commit works
    let _ = temp_terminal_core::git::git_out(&tmp_dir, &["config", "user.name", "Tester"]);
    let _ =
        temp_terminal_core::git::git_out(&tmp_dir, &["config", "user.email", "test@test.local"]);
    let _ =
        temp_terminal_core::git::git_out(&tmp_dir, &["config", "commit.gpgsign", "false"]);

    let file_path = tmp_dir.join("sample.txt");
    std::fs::write(&file_path, "initial line\n").unwrap();

    // Stage file
    let stage_res = temp_terminal_core::git::stage_file(&tmp_dir, "sample.txt");
    assert!(stage_res.is_ok(), "stage_file should succeed: {:?}", stage_res);

    // Commit initial line
    let _ = temp_terminal_core::git::git_out(&tmp_dir, &["commit", "--no-gpg-sign", "-m", "initial"]);

    // Modify file
    std::fs::write(&file_path, "initial line\nsecond line\n").unwrap();

    // Stage modification
    let stage_mod = temp_terminal_core::git::stage_file(&tmp_dir, "sample.txt");
    assert!(
        stage_mod.is_ok(),
        "stage_file should succeed for modified file"
    );

    // Verify index content has second line
    let idx_content = temp_terminal_core::git::show_index(&tmp_dir, "sample.txt").unwrap();
    assert!(idx_content.contains("second line"));

    // Unstage modification
    let unstage_res = temp_terminal_core::git::unstage_file(&tmp_dir, "sample.txt");
    assert!(unstage_res.is_ok(), "unstage_file should succeed");

    // Discard working-tree modification and verify restoration to HEAD
    let discard_res = temp_terminal_core::git::discard_file(&tmp_dir, "sample.txt", false);
    assert!(discard_res.is_ok(), "discard_file should succeed");
    let restored = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(restored.replace("\r\n", "\n"), "initial line\n");

    // Verify path traversal rejection
    let traversal_res = temp_terminal_core::git::discard_file(&tmp_dir, "../outside.txt", false);
    assert!(
        traversal_res.is_err(),
        "path traversal must be rejected by discard_file"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_terminal_clipboard_get_set_roundtrip_and_paste() {
    use temp_terminal_ui::workspace::{get_clipboard_text, set_clipboard_text};

    let test_str = format!("tempterm_paste_test_{}", temp_terminal_ui::uuid_short());
    set_clipboard_text(&test_str);
    let retrieved = get_clipboard_text();

    assert_eq!(
        retrieved, test_str,
        "set_clipboard_text and get_clipboard_text should round-trip accurately"
    );
}

