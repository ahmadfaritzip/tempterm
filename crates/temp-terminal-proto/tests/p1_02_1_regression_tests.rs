// Regression test suite covering P1-02.1 targeted corrections:
// - IPv6 ULA address compatibility (fc00::/7)
// - JSON omitempty compatibility
// - Clean EOF vs Truncated Header vs Truncated Payload distinctions
// - Incremental bounded allocation safety on large declared payloads

use std::io::Cursor;
use temp_terminal_proto::frame::{read_frame, FrameError, FRAME_INPUT, FRAME_OUTPUT, MAX_FRAME};
use temp_terminal_proto::proto::{Hello, Message, ReadParams, SendParams, Welcome};
use temp_terminal_proto::url::{browser_url, is_address};

#[test]
fn test_correction_a_ipv6_ula_and_ip_classifications() {
    // 1. IPv6 Unique Local Addresses (fc00::/7) - RFC 4193
    // Go net.IP.IsPrivate() returns true for these, so BrowserURL must format them as http://
    let ula_cases = [
        "fc00::1",
        "fc00:1234:5678::1",
        "fd00::1",
        "fd12:3456:789a:1::1",
        "[fc00::1]:8080",
        "[fd00::1]:3000",
    ];

    for case in ula_cases {
        assert!(is_address(case), "expected is_address({}) to be true", case);
        assert_eq!(
            browser_url(case),
            format!("http://{}", case),
            "expected ULA address {} to resolve to http://",
            case
        );
    }

    // 2. Loopback and unspecified IPv6
    assert!(is_address("::1"));
    assert_eq!(browser_url("::1"), "http://::1");
    assert!(is_address("[::1]:8080"));
    assert_eq!(browser_url("[::1]:8080"), "http://[::1]:8080");
    assert!(is_address("::"));
    assert_eq!(browser_url("::"), "http://::");

    // 3. Public IPv6 address should NOT be treated as local host
    // e.g., 2001:db8::1 (or general public 2606:4700:4700::1111)
    // Go's address() has:
    // case localHost(host): return "http://" + text, true
    // case strings.Contains(host, ".") && ...: return "https://" + text, true
    // A public IPv6 without dot or scheme does not match either and falls back to searchFor!
    assert!(!is_address("2606:4700:4700::1111"));
    assert!(browser_url("2606:4700:4700::1111").contains("duckduckgo.com"));

    // But with http:// scheme, it stays as is
    assert!(is_address("http://[2606:4700:4700::1111]"));
    assert_eq!(
        browser_url("http://[2606:4700:4700::1111]"),
        "http://[2606:4700:4700::1111]"
    );
}

#[test]
fn test_correction_b_json_omitempty_parity() {
    // 1. ReadParams omitempty: scrollback: false, lines: 0 must be omitted!
    let rp_default = ReadParams {
        id: "pane-1".into(),
        scrollback: false,
        lines: 0,
    };
    let json_rp = serde_json::to_string(&rp_default).unwrap();
    assert_eq!(json_rp, r#"{"id":"pane-1"}"#);

    // If set to non-zero/true, they must be present
    let rp_set = ReadParams {
        id: "pane-1".into(),
        scrollback: true,
        lines: 50,
    };
    let json_rp_set = serde_json::to_string(&rp_set).unwrap();
    assert!(json_rp_set.contains(r#""scrollback":true"#));
    assert!(json_rp_set.contains(r#""lines":50"#));

    // 2. Hello omitempty: client and pane are empty
    let hello = Hello {
        kind: "control".into(),
        version: 1,
        client: "".into(),
        pane: "".into(),
    };
    assert_eq!(
        serde_json::to_string(&hello).unwrap(),
        r#"{"kind":"control","version":1}"#
    );

    // 3. Welcome omitempty: app, error, cols, rows zero/empty
    let welcome = Welcome {
        version: 1,
        pid: 100,
        started: 12345678,
        app: "".into(),
        error: "".into(),
        cols: 0,
        rows: 0,
    };
    assert_eq!(
        serde_json::to_string(&welcome).unwrap(),
        r#"{"version":1,"pid":100,"started":12345678}"#
    );

    // 4. Message omitempty: id 0, result None, error empty, etc.
    let event_msg = Message {
        id: 0,
        result: None,
        error: "".into(),
        code: "".into(),
        event: "state".into(),
        state: None,
    };
    assert_eq!(
        serde_json::to_string(&event_msg).unwrap(),
        r#"{"event":"state"}"#
    );

    // 5. SendParams: paste false
    let send = SendParams {
        id: "p1".into(),
        data: "ls\n".into(),
        paste: false,
    };
    assert_eq!(
        serde_json::to_string(&send).unwrap(),
        r#"{"id":"p1","data":"ls\n"}"#
    );
}

#[test]
fn test_correction_c_clean_eof_and_truncation_distinction() {
    // 1. Clean EOF before frame starts
    let empty: &[u8] = &[];
    let mut cursor = Cursor::new(empty);
    let err = read_frame(&mut cursor).unwrap_err();
    assert!(matches!(err, FrameError::CleanEof));

    // 2. Truncated header (e.g. 1 to 4 bytes only)
    for len in 1..=4 {
        let partial_header = vec![FRAME_INPUT; len];
        let mut cursor = Cursor::new(partial_header);
        let err = read_frame(&mut cursor).unwrap_err();
        assert!(matches!(err, FrameError::TruncatedHeader));
    }

    // 3. Truncated payload: 5-byte header declaring 10 bytes, but only 3 available
    let truncated_stream = vec![FRAME_OUTPUT, 0, 0, 0, 10, b'a', b'b', b'c'];
    let mut cursor = Cursor::new(truncated_stream);
    let err = read_frame(&mut cursor).unwrap_err();
    assert!(matches!(err, FrameError::TruncatedPayload));
}

#[test]
fn test_correction_d_bounded_allocation_under_large_payload() {
    // Malicious 5-byte header claiming maximum allowed frame size (16 MiB = 16777216 bytes),
    // but the stream has 0 payload bytes.
    // Our incremental chunk-reading implementation should not pre-allocate 16 MiB.
    let mut hdr = vec![FRAME_INPUT];
    hdr.extend_from_slice(&(MAX_FRAME as u32).to_be_bytes());

    let mut cursor = Cursor::new(hdr);
    let err = read_frame(&mut cursor).unwrap_err();
    assert!(matches!(err, FrameError::TruncatedPayload));
}
