// Comprehensive unit test suite for temp-terminal-proto

use std::io::Cursor;
use temp_terminal_proto::frame::{
    append_frame, parse_uint16s, read_frame, uint16s, FrameError, FRAME_CLOSED, FRAME_INPUT,
    FRAME_KEY, FRAME_OUTPUT, FRAME_PASTE, FRAME_RESIZE, FRAME_RESIZED, FRAME_THEME, MAX_FRAME,
};
use temp_terminal_proto::proto::{read_line, AgentSessionParams, BrowserParams, CreateParams};
use temp_terminal_proto::url::{browser_url, set_search_engine, url_host, valid_search_url};

#[test]
fn test_all_frame_roundtrips() {
    let test_frames = [
        (
            FRAME_OUTPUT,
            &b"terminal vt stream output \x1b[31mred\x1b[0m"[..],
        ),
        (FRAME_INPUT, &b"ls -la\r"[..]),
        (FRAME_RESIZE, &[0, 80, 0, 24, 3, 32, 1, 224][..]),
        (FRAME_RESIZED, &[0, 80, 0, 24][..]),
        (FRAME_CLOSED, &b""[..]),
        (FRAME_THEME, &[0u8; 76][..]),
        (FRAME_KEY, &b"ctrl+c"[..]),
        (FRAME_PASTE, &b"some pasted text\nwith newlines"[..]),
    ];

    for (kind, payload) in test_frames {
        let mut buf = Vec::new();
        append_frame(&mut buf, kind, payload);

        let mut cursor = Cursor::new(&buf);
        let frame = read_frame(&mut cursor).expect("read frame successfully");
        assert_eq!(frame.kind, kind);
        assert_eq!(frame.payload.as_ref(), payload);
    }
}

#[test]
fn test_frame_bounds_and_errors() {
    // Exact max frame size should be permitted by header validation
    let mut buf = vec![FRAME_OUTPUT];
    buf.extend_from_slice(&(MAX_FRAME as u32).to_be_bytes());
    let mut cursor = Cursor::new(buf);
    // Cursor doesn't have 16MB, so should return TruncatedPayload
    assert!(matches!(
        read_frame(&mut cursor).unwrap_err(),
        FrameError::TruncatedPayload
    ));

    // Exceeding max frame size returns FrameTooLarge
    let mut buf2 = vec![FRAME_OUTPUT];
    buf2.extend_from_slice(&((MAX_FRAME as u32) + 1).to_be_bytes());
    let mut cursor2 = Cursor::new(buf2);
    assert!(matches!(
        read_frame(&mut cursor2).unwrap_err(),
        FrameError::FrameTooLarge
    ));
}

#[test]
fn test_uint16_codec_edge_cases() {
    let vals = [0, 1, 65535, 70000]; // 70000 should clamp to 65535
    let encoded = uint16s(&vals);
    assert_eq!(encoded.len(), 8);

    let (decoded, ok) = parse_uint16s(&encoded, 4);
    assert!(ok);
    assert_eq!(decoded, vec![0, 1, 65535, 65535]);

    // Wrong length returns false
    let (_, ok_bad) = parse_uint16s(&encoded, 3);
    assert!(!ok_bad);
}

#[test]
fn test_read_line_limits() {
    let data = b"line 1\nline 2 with more text\n";
    let mut cursor = Cursor::new(&data[..]);

    let line1 = read_line(&mut cursor, 10).expect("read line 1");
    assert_eq!(line1, b"line 1\n");

    // Reading with small limit should fail with line too long
    let err = read_line(&mut cursor, 5).unwrap_err();
    assert_eq!(err.to_string(), "proto: line too long");
}

#[test]
fn test_json_rpc_all_params_roundtrip() {
    // CreateParams
    let cp = CreateParams {
        project: "proj-1".into(),
        name: "test-session".into(),
        dir: "/tmp/work".into(),
        command: Some(vec!["bash".into(), "-l".into()]),
        env: Some(vec!["FOO=BAR".into()]),
        focus: true,
        url: "http://localhost:3000".into(),
        split: "pane-1".into(),
        direction: "right".into(),
        ratio: 0.5,
        worktree: Some(temp_terminal_proto::proto::WorktreeParams {
            branch: "feat-x".into(),
            base: "main".into(),
            no_env: true,
        }),
        by_pane: "pane-0".into(),
    };
    let json_cp = serde_json::to_string(&cp).unwrap();
    let decoded_cp: CreateParams = serde_json::from_str(&json_cp).unwrap();
    assert_eq!(cp, decoded_cp);

    // AgentSessionParams & Result
    let asp = AgentSessionParams {
        pid: 1234,
        agent: "claude".into(),
        session: "sess-99".into(),
        dir: "/home/user".into(),
    };
    let json_asp = serde_json::to_string(&asp).unwrap();
    let decoded_asp: AgentSessionParams = serde_json::from_str(&json_asp).unwrap();
    assert_eq!(asp, decoded_asp);

    // BrowserParams & Result
    let bp = BrowserParams {
        id: "b-1".into(),
        op: "click".into(),
        selector: "#submit".into(),
        text: "hello".into(),
        enter: true,
        timeout_ms: 5000,
    };
    let json_bp = serde_json::to_string(&bp).unwrap();
    let decoded_bp: BrowserParams = serde_json::from_str(&json_bp).unwrap();
    assert_eq!(bp, decoded_bp);
}

#[test]
fn test_search_engine_custom_config() {
    assert!(valid_search_url("https://example.com/search?q=%s"));
    assert!(!valid_search_url("https://example.com/search?q=nos"));
    assert!(!valid_search_url("example.com?q=%s")); // missing scheme

    set_search_engine("google");
    assert_eq!(
        browser_url("rust lang"),
        "https://www.google.com/search?q=rust+lang"
    );

    set_search_engine("brave");
    assert_eq!(
        browser_url("rust lang"),
        "https://search.brave.com/search?q=rust+lang"
    );

    set_search_engine("kagi");
    assert_eq!(
        browser_url("rust lang"),
        "https://kagi.com/search?q=rust+lang"
    );

    // Reset back to default duckduckgo
    set_search_engine("duckduckgo");
    assert_eq!(
        browser_url("rust lang"),
        "https://duckduckgo.com/?q=rust+lang"
    );
}

#[test]
fn test_url_host() {
    assert_eq!(
        url_host("https://www.google.com/search?q=test"),
        "google.com"
    );
    assert_eq!(url_host("http://localhost:3000/app"), "localhost:3000");
    assert_eq!(url_host("not-a-url"), "not-a-url");
}
