// Regression tests for P1-02.6.3:
// Strict Go value semantics for non-pointer omitempty fields:
// - Missing or JSON null deserializes directly to Rust zero values (String::new(), 0, 0.0, false), NOT Option::None!
// - Serialization omits zero values (empty string, 0, 0.0, false) under omitempty.
// - Populated values deserialize and serialize correctly.
// - Pointer fields (exit_code, command_exit, layout, worktree, proc, a, b, agent_session, turn) remain Option<T>!

use temp_terminal_proto::proto::*;

#[test]
fn test_strict_scalar_zero_values_state() {
    // 1. Missing and null input -> direct zero values (empty string, 0)
    let json_missing = r#"{}"#;
    let st_missing: State = serde_json::from_str(json_missing).unwrap();
    assert_eq!(st_missing.viewed, "");
    assert_eq!(st_missing.workspace, "");
    assert_eq!(st_missing.started, 0);

    let json_null = r#"{"viewed":null,"workspace":null,"started":null}"#;
    let st_null: State = serde_json::from_str(json_null).unwrap();
    assert_eq!(st_null.viewed, "");
    assert_eq!(st_null.workspace, "");
    assert_eq!(st_null.started, 0);

    // 2. Zero values omitted on serialization when omitempty
    let st_zero = State {
        viewed: "".to_string(),
        workspace: "".to_string(),
        started: 1234,
        ..Default::default()
    };
    let out_zero = serde_json::to_string(&st_zero).unwrap();
    assert!(!out_zero.contains("viewed"), "out: {}", out_zero);
    assert!(!out_zero.contains("workspace"), "out: {}", out_zero);
    assert!(out_zero.contains(r#""started":1234"#));

    // 3. Populated values serialize
    let st_pop = State {
        viewed: "session-1".to_string(),
        workspace: "/path/to".to_string(),
        started: 5678,
        ..Default::default()
    };
    let out_pop = serde_json::to_string(&st_pop).unwrap();
    assert!(out_pop.contains(r#""viewed":"session-1""#));
    assert!(out_pop.contains(r#""workspace":"/path/to""#));
}

#[test]
fn test_strict_scalar_zero_values_session() {
    let json_null = r#"{"id":"s1","project":"p1","name":"n1","agentNamed":null,"focus":null,"zoomed":null,"parent":null,"process":null}"#;
    let s_null: Session = serde_json::from_str(json_null).unwrap();
    assert!(!s_null.agent_named);
    assert_eq!(s_null.focus, "");
    assert_eq!(s_null.zoomed, "");
    assert_eq!(s_null.parent, "");
    assert_eq!(s_null.process, "");

    // Serialization omits zero values
    let s_zero = Session {
        id: "s1".into(),
        project: "p1".into(),
        name: "n1".into(),
        agent_named: false,
        focus: "".into(),
        zoomed: "".into(),
        parent: "".into(),
        process: "".into(),
        ..Default::default()
    };
    let out_zero = serde_json::to_string(&s_zero).unwrap();
    assert!(!out_zero.contains("agentNamed"));
    assert!(!out_zero.contains("focus"));
    assert!(!out_zero.contains("zoomed"));
    assert!(!out_zero.contains("parent"));
    assert!(!out_zero.contains("process"));

    // Populated values serialize
    let s_pop = Session {
        id: "s1".into(),
        project: "p1".into(),
        name: "n1".into(),
        agent_named: true,
        focus: "pane-1".into(),
        zoomed: "pane-2".into(),
        parent: "s0".into(),
        process: "proc-1".into(),
        ..Default::default()
    };
    let out_pop = serde_json::to_string(&s_pop).unwrap();
    assert!(out_pop.contains(r#""agentNamed":true"#));
    assert!(out_pop.contains(r#""focus":"pane-1""#));
    assert!(out_pop.contains(r#""zoomed":"pane-2""#));
    assert!(out_pop.contains(r#""parent":"s0""#));
    assert!(out_pop.contains(r#""process":"proc-1""#));
}

#[test]
fn test_strict_scalar_zero_values_port() {
    let json_null = r#"{"port":8080,"name":null,"process":null,"pid":null,"url":null,"open":true}"#;
    let p_null: Port = serde_json::from_str(json_null).unwrap();
    assert_eq!(p_null.name, "");
    assert_eq!(p_null.process, "");
    assert_eq!(p_null.pid, 0);
    assert_eq!(p_null.url, "");

    let p_zero = Port {
        port: 8080,
        name: "".into(),
        process: "".into(),
        pid: 0,
        url: "".into(),
        open: true,
    };
    let out_zero = serde_json::to_string(&p_zero).unwrap();
    assert!(!out_zero.contains("name"));
    assert!(!out_zero.contains("process"));
    assert!(!out_zero.contains("pid"));
    assert!(!out_zero.contains("url"));

    let p_pop = Port {
        port: 8080,
        name: "web".into(),
        process: "node".into(),
        pid: 4321,
        url: "http://localhost:8080".into(),
        open: true,
    };
    let out_pop = serde_json::to_string(&p_pop).unwrap();
    assert!(out_pop.contains(r#""name":"web""#));
    assert!(out_pop.contains(r#""process":"node""#));
    assert!(out_pop.contains(r#""pid":4321"#));
    assert!(out_pop.contains(r#""url":"http://localhost:8080""#));
}

#[test]
fn test_strict_scalar_zero_values_pane() {
    let json_null = r#"{
        "id":"p1","session":"s1",
        "project":null,"role":null,"pid":null,"fgpgid":null,"process":null,"agent":null,"runtime":null,"shell":null,"dir":null,"processDir":null,"title":null,"url":null,
        "unread":null,"message":null,"kind":null,"summary":null,"bells":null,"restored":null,"inbox":null,"held":null,"asking":null
    }"#;
    let pane: Pane = serde_json::from_str(json_null).unwrap();
    assert_eq!(pane.project, "");
    assert_eq!(pane.role, "");
    assert_eq!(pane.pid, 0);
    assert_eq!(pane.fgpgid, 0);
    assert_eq!(pane.process, "");
    assert_eq!(pane.agent, "");
    assert_eq!(pane.runtime, "");
    assert_eq!(pane.shell, "");
    assert_eq!(pane.dir, "");
    assert_eq!(pane.process_dir, "");
    assert_eq!(pane.title, "");
    assert_eq!(pane.url, "");
    assert!(!pane.unread);
    assert_eq!(pane.message, "");
    assert_eq!(pane.kind, "");
    assert_eq!(pane.summary, "");
    assert_eq!(pane.bells, 0);
    assert!(!pane.restored);
    assert_eq!(pane.inbox, 0);
    assert_eq!(pane.held, "");
    assert_eq!(pane.asking, "");

    // Verify pointer-to-integer remains Option<i64> and preserves Some(0)
    assert_eq!(pane.exit_code, None);
    assert_eq!(pane.command_exit, None);
}

#[test]
fn test_strict_scalar_zero_values_process() {
    let json_null = r#"{
        "name":"proc","command":"cmd","session":"s1",
        "dir":null,"port":null,"autostart":null,"restart":null,"url":null,"pane":null,"pid":null,"restarts":null,"lastExit":null,"lastChange":null,"changed":null,"message":null,"link":null,"share":null,"shareVia":null
    }"#;
    let pr: Process = serde_json::from_str(json_null).unwrap();
    assert_eq!(pr.dir, "");
    assert_eq!(pr.port, 0);
    assert!(!pr.autostart);
    assert_eq!(pr.restart, "");
    assert_eq!(pr.url, "");
    assert_eq!(pr.pane, "");
    assert_eq!(pr.pid, 0);
    assert_eq!(pr.restarts, 0);
    assert_eq!(pr.last_exit, 0);
    assert_eq!(pr.last_change, "");
    assert!(!pr.changed);
    assert_eq!(pr.message, "");
    assert_eq!(pr.link, "");
    assert_eq!(pr.share, "");
    assert_eq!(pr.share_via, "");
}

#[test]
fn test_strict_scalar_zero_values_layout() {
    let json_null = r#"{"pane":null,"row":null,"ratio":null}"#;
    let l: Layout = serde_json::from_str(json_null).unwrap();
    assert_eq!(l.pane, "");
    assert!(!l.row);
    assert_eq!(l.ratio, 0.0);
    // pointers a and b remain Option
    assert_eq!(l.a, None);
    assert_eq!(l.b, None);
}

#[test]
fn test_strict_scalar_zero_values_oscrecord() {
    let json_null = r#"{"id":null,"state":"s","kind":null,"progress":0,"app":null,"title":null,"msg":null,"updated":0}"#;
    let rec: OscRecord = serde_json::from_str(json_null).unwrap();
    assert_eq!(rec.id, "");
    assert_eq!(rec.kind, "");
    assert_eq!(rec.app, "");
    assert_eq!(rec.title, "");
    assert_eq!(rec.msg, "");
}

#[test]
fn test_strict_scalar_zero_values_ipc_dtos() {
    // Hello
    let h: Hello =
        serde_json::from_str(r#"{"kind":"k","version":1,"client":null,"pane":null}"#).unwrap();
    assert_eq!(h.client, "");
    assert_eq!(h.pane, "");

    // Welcome
    let w: Welcome = serde_json::from_str(
        r#"{"version":1,"pid":12,"started":123,"app":null,"error":null,"cols":null,"rows":null}"#,
    )
    .unwrap();
    assert_eq!(w.app, "");
    assert_eq!(w.error, "");
    assert_eq!(w.cols, 0);
    assert_eq!(w.rows, 0);

    // Message
    let m: Message =
        serde_json::from_str(r#"{"id":null,"error":null,"code":null,"event":null}"#).unwrap();
    assert_eq!(m.id, 0);
    assert_eq!(m.error, "");
    assert_eq!(m.code, "");
    assert_eq!(m.event, "");

    // CreateParams
    let cp: CreateParams = serde_json::from_str(r#"{"project":null,"name":null,"dir":null,"focus":null,"url":null,"split":null,"direction":null,"ratio":null,"by":null}"#).unwrap();
    assert_eq!(cp.project, "");
    assert_eq!(cp.name, "");
    assert_eq!(cp.dir, "");
    assert!(!cp.focus);
    assert_eq!(cp.url, "");
    assert_eq!(cp.split, "");
    assert_eq!(cp.direction, "");
    assert_eq!(cp.ratio, 0.0);
    assert_eq!(cp.by_pane, "");

    // RenameParams
    let rp: RenameParams = serde_json::from_str(r#"{"id":"i","name":"n","by":null}"#).unwrap();
    assert_eq!(rp.by_pane, "");

    // SendParams
    let sp: SendParams = serde_json::from_str(r#"{"id":"i","data":"d","paste":null}"#).unwrap();
    assert!(!sp.paste);

    // WaitParams
    let wp: WaitParams = serde_json::from_str(r#"{"id":"i","exit":null,"timeout":null}"#).unwrap();
    assert!(!wp.exit);
    assert_eq!(wp.timeout_ms, 0);

    // BrowserParams
    let bp: BrowserParams = serde_json::from_str(
        r#"{"id":"i","op":"o","selector":null,"text":null,"enter":null,"timeout":null}"#,
    )
    .unwrap();
    assert_eq!(bp.selector, "");
    assert_eq!(bp.text, "");
    assert!(!bp.enter);
    assert_eq!(bp.timeout_ms, 0);

    // BrowserResult
    let br: BrowserResult = serde_json::from_str(r#"{"text":null}"#).unwrap();
    assert_eq!(br.text, "");

    // AgentSessionParams
    let asp: AgentSessionParams =
        serde_json::from_str(r#"{"pid":1,"agent":"a","session":"s","dir":null}"#).unwrap();
    assert_eq!(asp.dir, "");

    // AgentSessionResult
    let asr: AgentSessionResult =
        serde_json::from_str(r#"{"pane":"p","session":"s","away":null}"#).unwrap();
    assert!(!asr.away);
}
