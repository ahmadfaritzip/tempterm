// Strict zero-value omission tests matching Go encoding/json `omitempty` behavior.

use temp_terminal_proto::proto::{
    AgentSessionParams, AgentSessionResult, BrowserParams, BrowserResult, CreateParams, Hello,
    Message, ReadParams, RenameParams, Request, SendParams, WaitParams, Welcome,
};

#[test]
fn test_strict_zero_value_omission_for_all_omitempty_fields() {
    // 1. Hello: "" for client and pane must be omitted
    let h_empty_strings = Hello {
        kind: "control".into(),
        version: 1,
        client: "".into(),
        pane: "".into(),
    };
    assert_eq!(
        serde_json::to_string(&h_empty_strings).unwrap(),
        r#"{"kind":"control","version":1}"#
    );

    // 2. Welcome: "" for strings, 0 for ints must be omitted
    let w_zeroes = Welcome {
        version: 1,
        pid: 123,
        started: 456,
        app: "".into(),
        error: "".into(),
        cols: 0,
        rows: 0,
    };
    assert_eq!(
        serde_json::to_string(&w_zeroes).unwrap(),
        r#"{"version":1,"pid":123,"started":456}"#
    );

    // 3. Request: None is omitted; Some(Null) and Some({}) are emitted as-is per Go RawMessage semantics
    let req_none = Request {
        id: 1,
        method: "list".into(),
        params: None,
    };
    assert_eq!(
        serde_json::to_string(&req_none).unwrap(),
        r#"{"id":1,"method":"list"}"#
    );

    let req_null = Request {
        id: 1,
        method: "list".into(),
        params: Some(serde_json::Value::Null),
    };
    assert_eq!(
        serde_json::to_string(&req_null).unwrap(),
        r#"{"id":1,"method":"list","params":null}"#
    );

    let req_empty_obj = Request {
        id: 1,
        method: "list".into(),
        params: Some(serde_json::json!({})),
    };
    assert_eq!(
        serde_json::to_string(&req_empty_obj).unwrap(),
        r#"{"id":1,"method":"list","params":{}}"#
    );

    // 4. Message: 0 for ID, "" for error/code/event omitted; None for result/state omitted;
    // but Some(Null) for result is emitted per Go RawMessage semantics,
    // and Some(State::default()) emits Go's exact non-nil zero State representation.
    let msg_zeroes = Message {
        id: 0,
        result: None,
        error: "".into(),
        code: "".into(),
        event: "".into(),
        state: None,
    };
    assert_eq!(serde_json::to_string(&msg_zeroes).unwrap(), r#"{}"#);

    let msg_with_nulls = Message {
        id: 1,
        result: Some(serde_json::Value::Null),
        error: "".into(),
        code: "".into(),
        event: "".into(),
        state: Some(temp_terminal_proto::proto::State::default()),
    };
    assert_eq!(
        serde_json::to_string(&msg_with_nulls).unwrap(),
        r#"{"id":1,"result":null,"state":{"projects":null,"sessions":null,"started":0}}"#
    );

    // 5. CreateParams: all empty strings, empty vecs, false, 0.0 must be omitted
    let create_zeroes = CreateParams {
        project: "".into(),
        name: "".into(),
        dir: "".into(),
        command: Some(vec![]),
        env: Some(vec![]),
        focus: false,
        url: "".into(),
        split: "".into(),
        direction: "".into(),
        ratio: 0.0,
        worktree: None,
        by_pane: "".into(),
    };
    assert_eq!(serde_json::to_string(&create_zeroes).unwrap(), r#"{}"#);

    // 6. RenameParams: by_pane "" must be omitted
    let rename_zero = RenameParams {
        id: "s1".into(),
        name: "main".into(),
        by_pane: "".into(),
    };
    assert_eq!(
        serde_json::to_string(&rename_zero).unwrap(),
        r#"{"id":"s1","name":"main"}"#
    );

    // 7. SendParams: paste false must be omitted
    let send_zero = SendParams {
        id: "p1".into(),
        data: "cmd\n".into(),
        paste: false,
    };
    assert_eq!(
        serde_json::to_string(&send_zero).unwrap(),
        r#"{"id":"p1","data":"cmd\n"}"#
    );

    // 8. ReadParams: scrollback false, lines 0 omitted
    let read_zero = ReadParams {
        id: "p1".into(),
        scrollback: false,
        lines: 0,
    };
    assert_eq!(serde_json::to_string(&read_zero).unwrap(), r#"{"id":"p1"}"#);

    // 9. WaitParams: empty status, false exit, 0 timeout omitted
    let wait_zero = WaitParams {
        id: "p1".into(),
        status: Some(vec![]),
        exit: false,
        timeout_ms: 0,
    };
    assert_eq!(serde_json::to_string(&wait_zero).unwrap(), r#"{"id":"p1"}"#);

    // 10. BrowserParams: empty selector/text, false enter, 0 timeout omitted
    let browser_zero = BrowserParams {
        id: "b1".into(),
        op: "click".into(),
        selector: "".into(),
        text: "".into(),
        enter: false,
        timeout_ms: 0,
    };
    assert_eq!(
        serde_json::to_string(&browser_zero).unwrap(),
        r#"{"id":"b1","op":"click"}"#
    );

    // 11. BrowserResult: empty text, empty png bytes omitted
    let browser_res_zero = BrowserResult {
        text: "".into(),
        png: Some(vec![]),
    };
    assert_eq!(serde_json::to_string(&browser_res_zero).unwrap(), r#"{}"#);

    // 12. AgentSessionParams: empty dir omitted
    let asp_zero = AgentSessionParams {
        pid: 100,
        agent: "test".into(),
        session: "s1".into(),
        dir: "".into(),
    };
    assert_eq!(
        serde_json::to_string(&asp_zero).unwrap(),
        r#"{"pid":100,"agent":"test","session":"s1"}"#
    );

    // 13. AgentSessionResult: false away omitted
    let asr_zero = AgentSessionResult {
        pane: "p1".into(),
        session: "s1".into(),
        away: false,
    };
    assert_eq!(
        serde_json::to_string(&asr_zero).unwrap(),
        r#"{"pane":"p1","session":"s1"}"#
    );
}

#[test]
fn test_presence_when_non_zero_values_provided() {
    let create_set = CreateParams {
        project: "proj-1".into(),
        name: "".into(),
        dir: "".into(),
        command: Some(vec!["ls".into(), "-la".into()]),
        env: None,
        focus: true,
        url: "".into(),
        split: "".into(),
        direction: "".into(),
        ratio: 0.5,
        worktree: None,
        by_pane: "".into(),
    };
    let json = serde_json::to_string(&create_set).unwrap();
    assert!(json.contains(r#""project":"proj-1""#));
    assert!(json.contains(r#""command":["ls","-la"]"#));
    assert!(json.contains(r#""focus":true"#));
    assert!(json.contains(r#""ratio":0.5"#));
    assert!(!json.contains("name"));
    assert!(!json.contains("dir"));
    assert!(!json.contains("split"));
}
