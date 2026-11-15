// Regression tests for P1-02.5:
// - Strongly-typed State model and Go JSON parity
// - nil vs empty slice distinction (projects, sessions vs ports)
// - omission of Message.state when None
// - rejection of invalid JSON shapes (string, array, number) for State

use temp_terminal_proto::proto::{
    Layout, Message, Pane, Port, Process, Project, Session, State, Worktree,
};

#[test]
fn test_state_message_nil_vs_zero_vs_empty() {
    // 1. Message.state == None -> omitted from wire format
    let msg_none = Message {
        event: "state".into(),
        state: None,
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&msg_none).unwrap(),
        r#"{"event":"state"}"#
    );

    // 2. Message.state == Some(zero State) -> Go nil slice: {"projects":null,"sessions":null,"started":0}
    let msg_zero = Message {
        event: "state".into(),
        state: Some(State::default()),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&msg_zero).unwrap(),
        r#"{"event":"state","state":{"projects":null,"sessions":null,"started":0}}"#
    );

    // 3. Message.state == Some(empty slices) -> Go empty slice: {"projects":[],"sessions":[],"started":0}
    let msg_empty = Message {
        event: "state".into(),
        state: Some(State {
            projects: Some(vec![]),
            sessions: Some(vec![]),
            ports: Some(vec![]), // ports has omitempty, so Some(vec![]) is omitted!
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&msg_empty).unwrap(),
        r#"{"event":"state","state":{"projects":[],"sessions":[],"started":0}}"#
    );
}

#[test]
fn test_state_populated_roundtrip() {
    let populated = State {
        projects: Some(vec![Project {
            id: "prj1".into(),
            name: "my-project".into(),
            dir: "/home/user/project".into(),
        }]),
        sessions: Some(vec![Session {
            id: "s1".into(),
            project: "prj1".into(),
            name: "main".into(),
            agent_named: true,
            focus: "p1".into(),
            zoomed: "".into(),
            layout: Some(Layout {
                pane: "p1".into(),
                row: false,
                ratio: 0.5,
                a: None,
                b: None,
            }),
            panes: Some(vec![Pane {
                id: "p1".into(),
                session: "s1".into(),
                status: "running".into(),
                since: 1000,
                activity: 1005,
                progress: -1,
                ..Default::default()
            }]),
            lead: Pane {
                id: "p1".into(),
                session: "s1".into(),
                status: "running".into(),
                since: 1000,
                activity: 1005,
                progress: -1,
                ..Default::default()
            },
            worktree: Some(Worktree {
                dir: "/tmp/wt".into(),
                branch: "feat".into(),
                base: "main".into(),
            }),
            parent: "".into(),
            process: "".into(),
            proc: Some(Process {
                name: "web".into(),
                command: "npm start".into(),
                dir: "".into(),
                env: None,
                port: 3000,
                autostart: true,
                restart: "".into(),
                watch: None,
                url: "web".into(),
                status: "running".into(),
                pane: "p1".into(),
                session: "s1".into(),
                pid: 1234,
                restarts: 0,
                last_exit: 0,
                last_change: "".into(),
                since: 1000,
                trusted: true,
                changed: false,
                message: "".into(),
                link: "https://web.localhost".into(),
                share: "".into(),
                share_via: "".into(),
            }),
        }]),
        viewed: "s1".into(),
        started: 1700000000000,
        workspace: "/workspace".into(),
        ports: Some(vec![Port {
            port: 3000,
            name: "web".into(),
            process: "node".into(),
            pid: 1234,
            url: "http://localhost:3000".into(),
            open: true,
        }]),
    };

    let msg = Message {
        event: "state".into(),
        state: Some(populated),
        ..Default::default()
    };

    let serialized = serde_json::to_string(&msg).unwrap();

    // Verify key elements are present in JSON
    assert!(serialized
        .contains(r#""projects":[{"id":"prj1","name":"my-project","dir":"/home/user/project"}]"#));
    assert!(serialized.contains(r#""viewed":"s1""#));
    assert!(serialized.contains(r#""started":1700000000000"#));
    assert!(serialized.contains(r#""workspace":"/workspace""#));
    assert!(serialized.contains(r#""ports":[{"port":3000,"name":"web","process":"node","pid":1234,"url":"http://localhost:3000","open":true}]"#));

    // Deserialization round trip
    let parsed: Message = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed.event, "state");
    let st = parsed.state.unwrap();
    assert_eq!(st.viewed, "s1");
    assert_eq!(st.started, 1700000000000);
    assert_eq!(st.projects.unwrap().len(), 1);
    assert_eq!(st.sessions.unwrap().len(), 1);
    assert_eq!(st.ports.unwrap().len(), 1);
}

#[test]
fn test_state_incoming_deserialization_edge_cases() {
    // 1. Missing state field -> None
    let m1: Message = serde_json::from_str(r#"{"event":"state"}"#).unwrap();
    assert_eq!(m1.state, None);

    // 2. "state": null -> None
    let m2: Message = serde_json::from_str(r#"{"event":"state","state":null}"#).unwrap();
    assert_eq!(m2.state, None);

    // 3. "state": {} -> Some(State with default values)
    let m3: Message = serde_json::from_str(r#"{"event":"state","state":{}}"#).unwrap();
    assert!(m3.state.is_some());
    let st3 = m3.state.unwrap();
    assert_eq!(st3.started, 0);
    assert_eq!(st3.projects, None);
    assert_eq!(st3.sessions, None);
    assert_eq!(st3.ports, None);

    // 4. "state": "invalid string" -> Deserialization error (not accepted as State)
    let err_str = serde_json::from_str::<Message>(r#"{"event":"state","state":"string"}"#);
    assert!(err_str.is_err());

    // 5. "state": [] -> Deserialization error (not accepted as State)
    let err_arr = serde_json::from_str::<Message>(r#"{"event":"state","state":[]}"#);
    assert!(err_arr.is_err());

    // 6. "state": 123 -> Deserialization error (not accepted as State)
    let err_num = serde_json::from_str::<Message>(r#"{"event":"state","state":123}"#);
    assert!(err_num.is_err());
}
