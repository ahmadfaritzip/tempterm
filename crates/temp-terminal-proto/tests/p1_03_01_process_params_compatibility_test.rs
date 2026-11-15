//! Golden and regression tests for P1-03 Step 1: ProcessParams wire DTO compatibility.

use temp_terminal_proto::proto::ProcessParams;

#[test]
fn test_process_params_all_fields_populated() {
    let p = ProcessParams {
        project: "proj-123".to_string(),
        name: "web-server".to_string(),
        via: "cloudflare".to_string(),
    };
    let json_str = serde_json::to_string(&p).expect("serialize all fields");
    assert_eq!(
        json_str,
        r#"{"project":"proj-123","name":"web-server","via":"cloudflare"}"#
    );

    let roundtrip: ProcessParams = serde_json::from_str(&json_str).expect("deserialize all fields");
    assert_eq!(roundtrip, p);
}

#[test]
fn test_process_params_project_only_omits_empty_name_and_via() {
    let p = ProcessParams {
        project: "my-project".to_string(),
        name: "".to_string(),
        via: "".to_string(),
    };
    let json_str = serde_json::to_string(&p).expect("serialize project only");
    // In Go, `json:"name,omitempty"` and `json:"via,omitempty"` omit empty strings
    // `json:"project"` without omitempty is never omitted.
    assert_eq!(json_str, r#"{"project":"my-project"}"#);

    let roundtrip: ProcessParams =
        serde_json::from_str(&json_str).expect("deserialize project only");
    assert_eq!(roundtrip.project, "my-project");
    assert_eq!(roundtrip.name, "");
    assert_eq!(roundtrip.via, "");
}

#[test]
fn test_process_params_empty_project_is_serialized_because_no_omitempty() {
    let p = ProcessParams {
        project: "".to_string(),
        name: "test".to_string(),
        via: "".to_string(),
    };
    let json_str = serde_json::to_string(&p).expect("serialize empty project");
    // `project` has NO omitempty tag in Go: `json:"project"`
    assert_eq!(json_str, r#"{"project":"","name":"test"}"#);
}

#[test]
fn test_process_params_deserialization_from_empty_object() {
    let json_str = r#"{}"#;
    let p: ProcessParams = serde_json::from_str(json_str).expect("deserialize empty JSON object");
    assert_eq!(p.project, "");
    assert_eq!(p.name, "");
    assert_eq!(p.via, "");
}

#[test]
fn test_process_params_deserialization_explicit_nulls() {
    let json_str = r#"{"project":null,"name":null,"via":null}"#;
    let p: ProcessParams = serde_json::from_str(json_str).expect("deserialize explicit nulls");
    assert_eq!(p.project, "");
    assert_eq!(p.name, "");
    assert_eq!(p.via, "");
}

#[test]
fn test_process_params_with_name_without_via() {
    let p = ProcessParams {
        project: "p1".to_string(),
        name: "worker".to_string(),
        via: "".to_string(),
    };
    let json_str = serde_json::to_string(&p).expect("serialize name without via");
    assert_eq!(json_str, r#"{"project":"p1","name":"worker"}"#);

    let deserialized: ProcessParams =
        serde_json::from_str(&json_str).expect("deserialize name without via");
    assert_eq!(deserialized, p);
}

#[test]
fn test_process_params_with_via_without_name() {
    let p = ProcessParams {
        project: "p1".to_string(),
        name: "".to_string(),
        via: "tailscale".to_string(),
    };
    let json_str = serde_json::to_string(&p).expect("serialize via without name");
    assert_eq!(json_str, r#"{"project":"p1","via":"tailscale"}"#);

    let deserialized: ProcessParams =
        serde_json::from_str(&json_str).expect("deserialize via without name");
    assert_eq!(deserialized, p);
}

#[test]
fn test_process_params_ignores_unknown_fields() {
    let json_str =
        r#"{"project":"p1","name":"web","via":"cloudflare","extra":123,"nested":{"a":true}}"#;
    let p: ProcessParams = serde_json::from_str(json_str).expect("deserialize with unknown fields");
    assert_eq!(p.project, "p1");
    assert_eq!(p.name, "web");
    assert_eq!(p.via, "cloudflare");
}
