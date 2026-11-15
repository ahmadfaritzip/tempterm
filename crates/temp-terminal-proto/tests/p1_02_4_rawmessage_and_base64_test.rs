// Regression tests for P1-02.4:
// - json.RawMessage semantics for Request.params, Message.result, Message.state
// - Base64 JSON encoding/decoding for BrowserResult.png

use temp_terminal_proto::base64::{decode, encode};
use temp_terminal_proto::proto::{BrowserResult, Message, Request};

#[test]
fn test_raw_message_semantics_request_params() {
    // 1. None => omitted
    let req_none = Request {
        id: 10,
        method: "test".into(),
        params: None,
    };
    assert_eq!(
        serde_json::to_string(&req_none).unwrap(),
        r#"{"id":10,"method":"test"}"#
    );

    // 2. Some(Value::Null) => emitted as null
    let req_null = Request {
        id: 11,
        method: "test".into(),
        params: Some(serde_json::Value::Null),
    };
    assert_eq!(
        serde_json::to_string(&req_null).unwrap(),
        r#"{"id":11,"method":"test","params":null}"#
    );

    // 3. Some(Value::Object(empty)) => emitted as {}
    let req_empty_obj = Request {
        id: 12,
        method: "test".into(),
        params: Some(serde_json::json!({})),
    };
    assert_eq!(
        serde_json::to_string(&req_empty_obj).unwrap(),
        r#"{"id":12,"method":"test","params":{}}"#
    );

    // 4. Some(Value::Array(empty)) => emitted as []
    let req_empty_arr = Request {
        id: 13,
        method: "test".into(),
        params: Some(serde_json::json!([])),
    };
    assert_eq!(
        serde_json::to_string(&req_empty_arr).unwrap(),
        r#"{"id":13,"method":"test","params":[]}"#
    );

    // 5. Some non-empty value => emitted normally
    let req_val = Request {
        id: 14,
        method: "test".into(),
        params: Some(serde_json::json!({"pane":"p1"})),
    };
    assert_eq!(
        serde_json::to_string(&req_val).unwrap(),
        r#"{"id":14,"method":"test","params":{"pane":"p1"}}"#
    );
}

#[test]
fn test_raw_message_semantics_message_result() {
    // 1. None => omitted
    let msg_none = Message {
        id: 1,
        result: None,
        ..Default::default()
    };
    assert_eq!(serde_json::to_string(&msg_none).unwrap(), r#"{"id":1}"#);

    // 2. Some(Value::Null) => emitted as null
    let msg_null = Message {
        id: 2,
        result: Some(serde_json::Value::Null),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&msg_null).unwrap(),
        r#"{"id":2,"result":null}"#
    );

    // 3. Some(Value::Object(empty)) => emitted as {}
    let msg_empty_obj = Message {
        id: 3,
        result: Some(serde_json::json!({})),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&msg_empty_obj).unwrap(),
        r#"{"id":3,"result":{}}"#
    );

    // 4. Some(Value::Array(empty)) => emitted as []
    let msg_empty_arr = Message {
        id: 4,
        result: Some(serde_json::json!([])),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&msg_empty_arr).unwrap(),
        r#"{"id":4,"result":[]}"#
    );
}

#[test]
fn test_base64_standalone_vectors() {
    // Standard RFC 4648 test vectors
    assert_eq!(encode(b""), "");
    assert_eq!(encode(b"f"), "Zg==");
    assert_eq!(encode(b"fo"), "Zm8=");
    assert_eq!(encode(b"foo"), "Zm9v");
    assert_eq!(encode(b"foob"), "Zm9vYg==");
    assert_eq!(encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(encode(b"foobar"), "Zm9vYmFy");

    assert_eq!(decode("").unwrap(), b"");
    assert_eq!(decode("Zg==").unwrap(), b"f");
    assert_eq!(decode("Zm8=").unwrap(), b"fo");
    assert_eq!(decode("Zm9v").unwrap(), b"foo");
    assert_eq!(decode("Zm9vYg==").unwrap(), b"foob");
    assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
    assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
}

#[test]
fn test_browser_result_png_base64_json_parity() {
    // 1. None => omitted
    let res_none = BrowserResult {
        text: "hello".into(),
        png: None,
    };
    assert_eq!(
        serde_json::to_string(&res_none).unwrap(),
        r#"{"text":"hello"}"#
    );

    // 2. Some(vec![]) => omitted with omitempty
    let res_empty = BrowserResult {
        text: "".into(),
        png: Some(vec![]),
    };
    assert_eq!(serde_json::to_string(&res_empty).unwrap(), r#"{}"#);

    // 3. Known binary sequence: [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] (PNG magic header)
    // 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A -> "iVBORw0KGgo="
    let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let res_png = BrowserResult {
        text: "".into(),
        png: Some(png_header.clone()),
    };
    let serialized = serde_json::to_string(&res_png).unwrap();
    assert_eq!(serialized, r#"{"png":"iVBORw0KGgo="}"#);

    // 4. Round-trip deserialization from JSON
    let deserialized: BrowserResult = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.png, Some(png_header));

    // 5. Deserialization from JSON null -> None
    let from_null: BrowserResult = serde_json::from_str(r#"{"png":null}"#).unwrap();
    assert_eq!(from_null.png, None);

    // 6. Deserialization when field missing -> None
    let from_missing: BrowserResult = serde_json::from_str(r#"{}"#).unwrap();
    assert_eq!(from_missing.png, None);
}
