// Golden compatibility fixtures and tests comparing against Go implementation contracts.

use bytes::Bytes;
use temp_terminal_proto::frame::{
    append_frame, parse_uint16s, read_frame, uint16s, FrameError, FRAME_CLOSED, FRAME_INPUT,
    FRAME_KEY, FRAME_OUTPUT, FRAME_PASTE, FRAME_RESIZE, FRAME_RESIZED, FRAME_THEME, MAX_FRAME,
};
use temp_terminal_proto::proto::{
    Hello, Message, ReadParams, ReadResult, Request, CODE_NOT_FOUND, PROTOCOL_VERSION,
};
use temp_terminal_proto::url::{browser_url, is_address, BLANK_URL};

#[test]
fn test_golden_binary_frames_exact_bytes() {
    // 1. FrameInput with "hi" -> [b'i', 0, 0, 0, 2, b'h', b'i']
    let mut buf = Vec::new();
    append_frame(&mut buf, FRAME_INPUT, b"hi");
    assert_eq!(buf, vec![b'i', 0, 0, 0, 2, b'h', b'i']);

    // 2. Decode using read_frame
    let mut cursor = std::io::Cursor::new(&buf);
    let frame = read_frame(&mut cursor).expect("read frame");
    assert_eq!(frame.kind, FRAME_INPUT);
    assert_eq!(frame.payload, Bytes::from_static(b"hi"));

    // 3. FrameResize with cols=80, rows=24, xpix=800, ypix=480
    // Uint16s(80, 24, 800, 480)
    let resize_payload = uint16s(&[80, 24, 800, 480]);
    assert_eq!(
        resize_payload,
        vec![
            0x00, 0x50, // 80
            0x00, 0x18, // 24
            0x03, 0x20, // 800
            0x01, 0xE0, // 480
        ]
    );

    let (dims, ok) = parse_uint16s(&resize_payload, 4);
    assert!(ok);
    assert_eq!(dims, vec![80, 24, 800, 480]);

    // 4. Test all frame type constants byte values match Go constants
    assert_eq!(FRAME_OUTPUT, b'o');
    assert_eq!(FRAME_INPUT, b'i');
    assert_eq!(FRAME_RESIZE, b'r');
    assert_eq!(FRAME_RESIZED, b'z');
    assert_eq!(FRAME_CLOSED, b'c');
    assert_eq!(FRAME_THEME, b't');
    assert_eq!(FRAME_KEY, b'k');
    assert_eq!(FRAME_PASTE, b'p');
}

#[test]
fn test_golden_frame_size_limits() {
    // Attempting to read a frame claiming > 16MB fails with FrameTooLarge
    let mut hdr = vec![FRAME_OUTPUT];
    hdr.extend_from_slice(&((MAX_FRAME as u32 + 1).to_be_bytes()));
    let mut cursor = std::io::Cursor::new(hdr);
    let err = read_frame(&mut cursor).unwrap_err();
    assert!(matches!(err, FrameError::FrameTooLarge));

    // A 5-byte message claiming 16MB fails with TruncatedPayload immediately without allocation
    let mut fake_hdr = vec![FRAME_INPUT];
    fake_hdr.extend_from_slice(&(16777215u32.to_be_bytes()));
    let mut cursor = std::io::Cursor::new(fake_hdr);
    let err = read_frame(&mut cursor).unwrap_err();
    assert!(matches!(err, FrameError::TruncatedPayload));
}

#[test]
fn test_golden_json_rpc_structures() {
    // Handshake: Hello
    let hello = Hello {
        kind: "control".into(),
        version: PROTOCOL_VERSION,
        client: "tempterm".into(),
        pane: "".into(),
    };
    let json_str = serde_json::to_string(&hello).unwrap();
    assert_eq!(
        json_str,
        r#"{"kind":"control","version":1,"client":"tempterm"}"#
    );

    // Roundtrip deserialize
    let decoded: Hello = serde_json::from_str(&json_str).unwrap();
    assert_eq!(decoded.kind, "control");
    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.client, "tempterm");
    assert_eq!(decoded.pane, "");

    // Request
    let req = Request {
        id: 42,
        method: "read".into(),
        params: Some(
            serde_json::to_value(ReadParams {
                id: "p1".into(),
                scrollback: true,
                lines: 100,
            })
            .unwrap(),
        ),
    };
    let req_json = serde_json::to_string(&req).unwrap();
    assert!(req_json.contains(r#""id":42"#));
    assert!(req_json.contains(r#""method":"read""#));
    assert!(req_json.contains(r#""scrollback":true"#));

    // Response Message
    let msg = Message {
        id: 42,
        result: Some(
            serde_json::to_value(ReadResult {
                text: "hello world\n".into(),
            })
            .unwrap(),
        ),
        error: "".into(),
        code: "".into(),
        event: "".into(),
        state: None,
    };
    let msg_json = serde_json::to_string(&msg).unwrap();
    assert_eq!(msg_json, r#"{"id":42,"result":{"text":"hello world\n"}}"#);

    // Error Message
    let err_msg = Message {
        id: 42,
        result: None,
        error: "pane not found".into(),
        code: CODE_NOT_FOUND.into(),
        event: "".into(),
        state: None,
    };
    let err_json = serde_json::to_string(&err_msg).unwrap();
    assert_eq!(
        err_json,
        r#"{"id":42,"error":"pane not found","code":"not_found"}"#
    );
}

#[test]
fn test_golden_browser_urls() {
    // Tests taken directly from tempterm/internal/proto/url_test.go TestBrowserURL
    let test_cases = [
        ("", BLANK_URL),
        ("  https://kero.sh/docs ", "https://kero.sh/docs"),
        ("about:blank", "about:blank"),
        ("file:///tmp/a.html", "file:///tmp/a.html"),
        ("localhost:3000", "http://localhost:3000"),
        ("localhost:5173/app?x=1", "http://localhost:5173/app?x=1"),
        ("localhost", "http://localhost"),
        ("127.0.0.1:8080", "http://127.0.0.1:8080"),
        ("[::1]:8080", "http://[::1]:8080"),
        ("app.localhost:3000", "http://app.localhost:3000"),
        ("192.168.1.20:8000", "http://192.168.1.20:8000"),
        ("kero.sh", "https://kero.sh"),
        ("github.com/egoist/kero", "https://github.com/egoist/kero"),
        (
            "how to quit vim",
            "https://duckduckgo.com/?q=how+to+quit+vim",
        ),
        ("golang", "https://duckduckgo.com/?q=golang"),
    ];

    for (input, expected) in test_cases {
        assert_eq!(browser_url(input), expected, "failed on input: {}", input);
    }

    // Tests taken directly from tempterm/internal/proto/url_test.go TestIsAddress
    let addr_cases = [
        ("localhost:3000", true),
        (" kero.sh ", true),
        ("https://x", true),
        ("about:blank", true),
        ("", false),
        ("split right", false),
        ("settings", false),
    ];

    for (input, expected) in addr_cases {
        assert_eq!(
            is_address(input),
            expected,
            "is_address failed on input: {}",
            input
        );
    }
}
