//! Tests for OSC scanner and status report parser matching Go `tempterm/internal/osc` tests.

use temp_terminal_proto::proto::OscRecord;
use temp_terminal_term::osc::{encode_status, parse_status, Scanner, MAX_LEN};
use temp_terminal_term::records::Records;

fn scan_all(input: &[u8], byte_at_a_time: bool) -> (Vec<String>, usize) {
    let mut s = Scanner::new();
    let mut bodies = Vec::new();
    let mut resets = 0;

    let mut on_osc = |b: &[u8]| {
        bodies.push(String::from_utf8_lossy(b).into_owned());
    };
    let mut on_reset = || {
        resets += 1;
    };

    if byte_at_a_time {
        for &byte in input {
            s.scan(&[byte], &mut on_osc, &mut on_reset);
        }
    } else {
        s.scan(input, &mut on_osc, &mut on_reset);
    }

    (bodies, resets)
}

#[test]
fn test_scanner_basic_and_split_chunks() {
    let input = b"hi\x1b]7501;state=working\x07 x \x1b]133;A\x1b\\\x1b]0;title\x07\x1b]9;4;1;40\x1b\\\x1bc\x9d133;D;0\x9c";
    let want = vec!["7501;state=working", "133;A", "9;4;1;40", "133;D;0"];

    for &byte_at_a_time in &[false, true] {
        let (bodies, resets) = scan_all(input, byte_at_a_time);
        assert_eq!(bodies, want, "byte_at_a_time: {}", byte_at_a_time);
        assert_eq!(resets, 1);
    }
}

#[test]
fn test_scanner_overflow_and_nested() {
    let long_payload = vec![b'A'; MAX_LEN + 10];
    let mut long = Vec::from(&b"\x1b]7501;state=working:msg="[..]);
    long.extend_from_slice(&long_payload);
    long.push(0x07);

    let (bodies, _) = scan_all(&long, false);
    assert!(bodies.is_empty(), "overlong sequence kept");

    let (nested_bodies, _) = scan_all(b"\x1b]7501;state=wor\x1b]133;A\x07", false);
    assert_eq!(nested_bodies, vec!["133;A"]);
}

#[test]
fn test_parse_status() {
    let b64 = |s: &str| temp_terminal_proto::base64::encode(s.as_bytes());

    let cases = vec![
        ("state=working".to_string(), true, "working", ""),
        (
            format!("state=blocked:kind=permission:msg={}", b64("Approve?")),
            true,
            "blocked",
            "Approve?",
        ),
        ("state=bogus".to_string(), false, "", ""),
        ("state=done:id=a/b/c".to_string(), true, "done", ""),
        (
            format!("state=done:id={}", "a/".repeat(9) + "a"),
            false,
            "",
            "",
        ),
        (
            format!("state=done:msg={}", b64("bad\x01ctl")),
            false,
            "",
            "",
        ),
        (
            format!("state=done:msg={}", b64("rtl\u{202E}evil")),
            true,
            "done",
            "rtlevil",
        ),
        (
            "state=working:junk:progress=40".to_string(),
            true,
            "working",
            "",
        ),
    ];

    for (input, ok, state, msg) in cases {
        let res = parse_status(&input);
        assert_eq!(res.is_some(), ok, "input: {}", input);
        if let Some(r) = res {
            assert_eq!(r.record.state, state, "state mismatch for {}", input);
            assert_eq!(r.record.msg, msg, "msg mismatch for {}", input);
        }
    }

    let progress_res = parse_status("state=working:progress=40").unwrap();
    assert_eq!(progress_res.record.progress, 40);

    let query_res = parse_status("?").unwrap();
    assert!(query_res.query);
}

#[test]
fn test_encode_and_parse_roundtrip() {
    let in_rec = OscRecord {
        state: "blocked".to_string(),
        kind: "question".to_string(),
        progress: 10,
        app: "claude".to_string(),
        msg: "Which one?".to_string(),
        id: "build/test".to_string(),
        ..Default::default()
    };
    let s = encode_status(&in_rec);
    let (bodies, _) = scan_all(s.as_bytes(), false);
    assert_eq!(bodies.len(), 1);

    let body = bodies[0].strip_prefix("7501;").unwrap();
    let parsed = parse_status(body).expect("parse status");
    assert_eq!(parsed.record.state, "blocked");
    assert_eq!(parsed.record.kind, "question");
    assert_eq!(parsed.record.msg, "Which one?");
    assert_eq!(parsed.record.id, "build/test");
    assert_eq!(parsed.record.progress, 10);
}

#[test]
fn test_records_collection_lifecycle() {
    let mut rs = Records::new();
    rs.apply(OscRecord {
        id: "build".to_string(),
        state: "working".to_string(),
        app: "make".to_string(),
        updated: 1,
        ..Default::default()
    });
    assert_eq!(rs.len(), 1);
    assert!(rs.declared());

    let lead = rs.lead().unwrap();
    assert_eq!(lead.id, "build");
    assert_eq!(lead.state, "working");

    rs.apply(OscRecord {
        id: "build".to_string(),
        state: "clear".to_string(),
        ..Default::default()
    });
    assert_eq!(rs.len(), 0);
}
