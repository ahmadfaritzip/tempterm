//! Regression tests for P1-02.7.1 ReadParams.lines signed integer parity.

use temp_terminal_proto::proto::ReadParams;

#[test]
fn test_read_params_missing_lines_deserializes_to_zero() {
    let json_str = r#"{"id":"p1"}"#;
    let p: ReadParams = serde_json::from_str(json_str).expect("deserialize missing lines");
    assert_eq!(p.id, "p1");
    assert_eq!(p.lines, 0);
    assert!(!p.scrollback);
}

#[test]
fn test_read_params_null_lines_deserializes_to_zero() {
    let json_str = r#"{"id":"p1","lines":null}"#;
    let p: ReadParams = serde_json::from_str(json_str).expect("deserialize null lines");
    assert_eq!(p.id, "p1");
    assert_eq!(p.lines, 0);
}

#[test]
fn test_read_params_zero_lines_deserializes_to_zero() {
    let json_str = r#"{"id":"p1","lines":0}"#;
    let p: ReadParams = serde_json::from_str(json_str).expect("deserialize zero lines");
    assert_eq!(p.id, "p1");
    assert_eq!(p.lines, 0);
}

#[test]
fn test_read_params_negative_lines_deserializes_successfully() {
    let json_str = r#"{"id":"p1","lines":-10}"#;
    let p: ReadParams = serde_json::from_str(json_str).expect("deserialize negative lines");
    assert_eq!(p.id, "p1");
    assert_eq!(p.lines, -10);
}

#[test]
fn test_read_params_positive_lines_deserializes_successfully() {
    let json_str = r#"{"id":"p1","lines":25}"#;
    let p: ReadParams = serde_json::from_str(json_str).expect("deserialize positive lines");
    assert_eq!(p.id, "p1");
    assert_eq!(p.lines, 25);
}

#[test]
fn test_read_params_zero_lines_omitted_from_json() {
    let p = ReadParams {
        id: "p1".to_string(),
        scrollback: false,
        lines: 0,
    };
    let out = serde_json::to_string(&p).expect("serialize zero lines");
    assert_eq!(out, r#"{"id":"p1"}"#);
}

#[test]
fn test_read_params_nonzero_lines_emitted_exact() {
    let p_neg = ReadParams {
        id: "p1".to_string(),
        scrollback: false,
        lines: -10,
    };
    let out_neg = serde_json::to_string(&p_neg).expect("serialize negative lines");
    assert_eq!(out_neg, r#"{"id":"p1","lines":-10}"#);

    let p_pos = ReadParams {
        id: "p1".to_string(),
        scrollback: false,
        lines: 25,
    };
    let out_pos = serde_json::to_string(&p_pos).expect("serialize positive lines");
    assert_eq!(out_pos, r#"{"id":"p1","lines":25}"#);
}
