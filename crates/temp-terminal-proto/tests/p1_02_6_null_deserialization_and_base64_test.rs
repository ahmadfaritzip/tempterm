use temp_terminal_proto::proto::*;

#[test]
fn test_base64_crlf_and_noncanonical_padding_bits_go_parity() {
    // 1. CRLF in base64 string
    let crlf_json = r#"{"png":"Z\r\nm\r\n8="}"#;
    let res: BrowserResult = serde_json::from_str(crlf_json).expect("should accept CRLF in base64");
    assert_eq!(res.png, Some(b"fo".to_vec()));

    // 2. Spaces and tabs rejected
    let space_json = r#"{"png":"Z m 8="}"#;
    assert!(serde_json::from_str::<BrowserResult>(space_json).is_err());
    let tab_json = r#"{"png":"Z\tm\t8="}"#;
    assert!(serde_json::from_str::<BrowserResult>(tab_json).is_err());

    // 3. Valid non-canonical trailing padding-bit examples accepted by Go base64.StdEncoding
    // "Zg==" -> 'f', "Zh==" -> 'f' (Go StdEncoding accepts "Zh==")
    let non_canon_1 = r#"{"png":"Zh=="}"#;
    let res1: BrowserResult = serde_json::from_str(non_canon_1).expect("should accept Zh==");
    assert_eq!(res1.png, Some(b"f".to_vec()));

    // "Zm8=" -> "fo", "Zm9=" -> "fo" (Go StdEncoding accepts "Zm9=")
    let non_canon_2 = r#"{"png":"Zm9="}"#;
    let res2: BrowserResult = serde_json::from_str(non_canon_2).expect("should accept Zm9=");
    assert_eq!(res2.png, Some(b"fo".to_vec()));
}

#[test]
fn test_go_null_and_missing_deserialization_non_pointer_fields() {
    // 1. Project: id, name, dir null or missing
    let p_null: Project = serde_json::from_str(r#"{"id": null, "name": null, "dir": null}"#)
        .expect("Project should accept null for string fields");
    assert_eq!(p_null, Project::default());
    let p_missing: Project =
        serde_json::from_str(r#"{}"#).expect("Project should accept missing fields");
    assert_eq!(p_missing, Project::default());

    // 2. Port: port, open null or missing
    let port_null: Port = serde_json::from_str(r#"{"port": null, "open": null}"#)
        .expect("Port should accept null for port and open");
    assert_eq!(port_null, Port::default());
    let port_missing: Port =
        serde_json::from_str(r#"{}"#).expect("Port should accept missing fields");
    assert_eq!(port_missing, Port::default());

    // 3. Session: id, project, name, lead null or missing
    let s_null: Session = serde_json::from_str(
        r#"{"id": null, "project": null, "name": null, "lead": null, "panes": null}"#,
    )
    .expect("Session should accept null for non-pointer fields and lead");
    assert_eq!(s_null.id, "");
    assert_eq!(s_null.project, "");
    assert_eq!(s_null.name, "");
    assert_eq!(s_null.lead, Pane::default());
    assert_eq!(s_null.panes, None);

    // 4. Pane: non-pointer required scalar and struct fields (id, session, since, activity, progress, etc.)
    let pane_null: Pane = serde_json::from_str(r#"{"id": null, "session": null, "status": null, "since": null, "activity": null, "progress": null}"#)
        .expect("Pane should accept null for non-pointer scalars");
    assert_eq!(pane_null, Pane::default());

    // 5. Process: required fields null
    let proc_null: Process = serde_json::from_str(r#"{"name": null, "command": null, "status": null, "session": null, "since": null, "trusted": null}"#)
        .expect("Process should accept null for non-pointer scalars");
    assert_eq!(proc_null, Process::default());

    // 6. Worktree: dir, branch, base null
    let wt_null: Worktree = serde_json::from_str(r#"{"dir": null, "branch": null, "base": null}"#)
        .expect("Worktree should accept null");
    assert_eq!(wt_null, Worktree::default());

    // 7. Turn: tree, root, at null
    let turn_null: Turn = serde_json::from_str(r#"{"tree": null, "root": null, "at": null}"#)
        .expect("Turn should accept null");
    assert_eq!(turn_null, Turn::default());

    // 8. AgentSession: agent, id null
    let ag_null: AgentSession = serde_json::from_str(r#"{"agent": null, "id": null}"#)
        .expect("AgentSession should accept null");
    assert_eq!(ag_null, AgentSession::default());

    // 9. OscRecord: state, progress, updated null
    let osc_null: OscRecord =
        serde_json::from_str(r#"{"state": null, "progress": null, "updated": null}"#)
            .expect("OscRecord should accept null");
    assert_eq!(osc_null, OscRecord::default());

    // 10. State: started null
    let st_null: State =
        serde_json::from_str(r#"{"started": null}"#).expect("State should accept null for started");
    assert_eq!(st_null.started, 0);
}

#[test]
fn test_session_panes_slice_nil_vs_empty_wire_parity() {
    // Slices in Go without omitempty (Session.Panes, State.Projects, State.Sessions):
    // nil -> serialized as null
    // [] -> serialized as []
    let s_nil = Session {
        panes: None,
        ..Default::default()
    };
    let json_nil = serde_json::to_string(&s_nil).unwrap();
    assert!(
        json_nil.contains(r#""panes":null"#),
        "nil slice should serialize to null: {}",
        json_nil
    );

    let s_empty = Session {
        panes: Some(Vec::new()),
        ..Default::default()
    };
    let json_empty = serde_json::to_string(&s_empty).unwrap();
    assert!(
        json_empty.contains(r#""panes":[]"#),
        "empty slice should serialize to []: {}",
        json_empty
    );

    // Incoming null deserializes to None (nil slice)
    let dec_nil: Session = serde_json::from_str(r#"{"panes": null}"#).unwrap();
    assert_eq!(dec_nil.panes, None);

    // Incoming [] deserializes to Some(empty)
    let dec_empty: Session = serde_json::from_str(r#"{"panes": []}"#).unwrap();
    assert_eq!(dec_empty.panes, Some(Vec::new()));
}
