// Regression tests for P1-02.6.2:
// - Pointer-to-integer fields (exitCode, commandExit) semantics
// - Process.env empty map omission
// - Collection edge cases (Pane.records, Pane.command, Process.watch)

use std::collections::BTreeMap;
use temp_terminal_proto::proto::{OscRecord, Pane, Process};

#[test]
fn test_pointer_to_integer_exit_code_and_command_exit() {
    // 1. None -> omitted
    let p_none = Pane {
        id: "p1".into(),
        session: "s1".into(),
        exit_code: None,
        command_exit: None,
        ..Default::default()
    };
    let json_none = serde_json::to_string(&p_none).unwrap();
    assert!(!json_none.contains("exitCode"));
    assert!(!json_none.contains("commandExit"));

    // 2. Some(0) -> emitted as 0 (CRITICAL POINTER ZERO-VALUE PARITY)
    let p_zero = Pane {
        id: "p1".into(),
        session: "s1".into(),
        exit_code: Some(0),
        command_exit: Some(0),
        ..Default::default()
    };
    let json_zero = serde_json::to_string(&p_zero).unwrap();
    assert!(json_zero.contains(r#""exitCode":0"#), "got: {}", json_zero);
    assert!(
        json_zero.contains(r#""commandExit":0"#),
        "got: {}",
        json_zero
    );

    // 3. Some(non-zero) -> emitted as number
    let p_val = Pane {
        id: "p1".into(),
        session: "s1".into(),
        exit_code: Some(7),
        command_exit: Some(9),
        ..Default::default()
    };
    let json_val = serde_json::to_string(&p_val).unwrap();
    assert!(json_val.contains(r#""exitCode":7"#), "got: {}", json_val);
    assert!(json_val.contains(r#""commandExit":9"#), "got: {}", json_val);

    // 4. Deserialization: missing and null -> None; 0 and non-zero -> Some
    let de_missing: Pane = serde_json::from_str(r#"{"id":"p1","session":"s1"}"#).unwrap();
    assert_eq!(de_missing.exit_code, None);
    assert_eq!(de_missing.command_exit, None);

    let de_null: Pane =
        serde_json::from_str(r#"{"id":"p1","session":"s1","exitCode":null,"commandExit":null}"#)
            .unwrap();
    assert_eq!(de_null.exit_code, None);
    assert_eq!(de_null.command_exit, None);

    let de_zero: Pane =
        serde_json::from_str(r#"{"id":"p1","session":"s1","exitCode":0,"commandExit":0}"#).unwrap();
    assert_eq!(de_zero.exit_code, Some(0));
    assert_eq!(de_zero.command_exit, Some(0));

    let de_val: Pane =
        serde_json::from_str(r#"{"id":"p1","session":"s1","exitCode":137,"commandExit":1}"#)
            .unwrap();
    assert_eq!(de_val.exit_code, Some(137));
    assert_eq!(de_val.command_exit, Some(1));
}

#[test]
fn test_process_env_empty_map_omission() {
    // 1. None -> omitted
    let proc_none = Process {
        name: "proc1".into(),
        command: "run".into(),
        session: "s1".into(),
        env: None,
        ..Default::default()
    };
    let json_none = serde_json::to_string(&proc_none).unwrap();
    assert!(!json_none.contains("env"));

    // 2. Some(empty map) -> omitted
    let proc_empty = Process {
        name: "proc1".into(),
        command: "run".into(),
        session: "s1".into(),
        env: Some(BTreeMap::new()),
        ..Default::default()
    };
    let json_empty = serde_json::to_string(&proc_empty).unwrap();
    assert!(
        !json_empty.contains("env"),
        "Empty env map should be omitted under omitempty! got: {}",
        json_empty
    );

    // 3. Some(populated map) -> emitted deterministically
    let mut map = BTreeMap::new();
    map.insert("A".into(), "1".into());
    map.insert("B".into(), "2".into());
    let proc_pop = Process {
        name: "proc1".into(),
        command: "run".into(),
        session: "s1".into(),
        env: Some(map.clone()),
        ..Default::default()
    };
    let json_pop = serde_json::to_string(&proc_pop).unwrap();
    assert!(
        json_pop.contains(r#""env":{"A":"1","B":"2"}"#),
        "got: {}",
        json_pop
    );

    // 4. Deserialization: missing, null, and empty map {}
    let de_missing: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s"}"#).unwrap();
    assert_eq!(de_missing.env, None);

    let de_null: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s","env":null}"#).unwrap();
    assert_eq!(de_null.env, None);

    let de_empty: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s","env":{}}"#).unwrap();
    assert_eq!(de_empty.env, Some(BTreeMap::new()));

    let de_pop: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s","env":{"A":"1","B":"2"}}"#)
            .unwrap();
    assert_eq!(de_pop.env, Some(map));
}

#[test]
fn test_collection_omitempty_records_command_watch() {
    // 1. Pane.records
    let p_records_none = Pane {
        id: "p".into(),
        session: "s".into(),
        records: None,
        ..Default::default()
    };
    assert!(!serde_json::to_string(&p_records_none)
        .unwrap()
        .contains("records"));

    let p_records_empty = Pane {
        id: "p".into(),
        session: "s".into(),
        records: Some(vec![]),
        ..Default::default()
    };
    assert!(!serde_json::to_string(&p_records_empty)
        .unwrap()
        .contains("records"));

    let p_records_pop = Pane {
        id: "p".into(),
        session: "s".into(),
        records: Some(vec![OscRecord {
            state: "running".into(),
            progress: 50,
            ..Default::default()
        }]),
        ..Default::default()
    };
    let json_rec = serde_json::to_string(&p_records_pop).unwrap();
    assert!(json_rec.contains(r#""records":[{"state":"running","progress":50,"updated":0}]"#));

    let de_rec_missing: Pane = serde_json::from_str(r#"{"id":"p","session":"s"}"#).unwrap();
    assert_eq!(de_rec_missing.records, None);
    let de_rec_null: Pane =
        serde_json::from_str(r#"{"id":"p","session":"s","records":null}"#).unwrap();
    assert_eq!(de_rec_null.records, None);
    let de_rec_empty: Pane =
        serde_json::from_str(r#"{"id":"p","session":"s","records":[]}"#).unwrap();
    assert_eq!(de_rec_empty.records, Some(vec![]));

    // 2. Pane.command
    let p_cmd_none = Pane {
        id: "p".into(),
        session: "s".into(),
        command: None,
        ..Default::default()
    };
    assert!(!serde_json::to_string(&p_cmd_none)
        .unwrap()
        .contains("command"));

    let p_cmd_empty = Pane {
        id: "p".into(),
        session: "s".into(),
        command: Some(vec![]),
        ..Default::default()
    };
    assert!(!serde_json::to_string(&p_cmd_empty)
        .unwrap()
        .contains("command"));

    let p_cmd_pop = Pane {
        id: "p".into(),
        session: "s".into(),
        command: Some(vec!["bash".into(), "-l".into()]),
        ..Default::default()
    };
    assert!(serde_json::to_string(&p_cmd_pop)
        .unwrap()
        .contains(r#""command":["bash","-l"]"#));

    let de_cmd_missing: Pane = serde_json::from_str(r#"{"id":"p","session":"s"}"#).unwrap();
    assert_eq!(de_cmd_missing.command, None);
    let de_cmd_null: Pane =
        serde_json::from_str(r#"{"id":"p","session":"s","command":null}"#).unwrap();
    assert_eq!(de_cmd_null.command, None);
    let de_cmd_empty: Pane =
        serde_json::from_str(r#"{"id":"p","session":"s","command":[]}"#).unwrap();
    assert_eq!(de_cmd_empty.command, Some(vec![]));

    // 3. Process.watch
    let pr_watch_none = Process {
        name: "p".into(),
        command: "c".into(),
        session: "s".into(),
        watch: None,
        ..Default::default()
    };
    assert!(!serde_json::to_string(&pr_watch_none)
        .unwrap()
        .contains("watch"));

    let pr_watch_empty = Process {
        name: "p".into(),
        command: "c".into(),
        session: "s".into(),
        watch: Some(vec![]),
        ..Default::default()
    };
    assert!(!serde_json::to_string(&pr_watch_empty)
        .unwrap()
        .contains("watch"));

    let pr_watch_pop = Process {
        name: "p".into(),
        command: "c".into(),
        session: "s".into(),
        watch: Some(vec!["*.go".into(), "*.rs".into()]),
        ..Default::default()
    };
    assert!(serde_json::to_string(&pr_watch_pop)
        .unwrap()
        .contains(r#""watch":["*.go","*.rs"]"#));

    let de_watch_missing: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s"}"#).unwrap();
    assert_eq!(de_watch_missing.watch, None);
    let de_watch_null: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s","watch":null}"#).unwrap();
    assert_eq!(de_watch_null.watch, None);
    let de_watch_empty: Process =
        serde_json::from_str(r#"{"name":"p","command":"c","session":"s","watch":[]}"#).unwrap();
    assert_eq!(de_watch_empty.watch, Some(vec![]));
}
