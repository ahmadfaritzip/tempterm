use std::collections::HashMap;
use temp_terminal_rig::{default_setting, Message, RigDatabase, SendMessageParams};
use tempfile::tempdir;

#[test]
fn test_message_sending_and_inbox_consumption() {
    let db = RigDatabase::open_in_memory().expect("open in memory db");

    let id1 = db
        .send_message(
            Some("TASK-101"),
            "agent-worker",
            "agent-reviewer",
            "Please review branch fix-issue",
            Some("story-alpha"),
        )
        .expect("send message 1");
    assert!(id1 > 0);

    let id2 = db
        .send_message(
            None,
            "agent-manager",
            "agent-reviewer",
            "Reminder about deadline",
            None,
        )
        .expect("send message 2");
    assert!(id2 > id1);

    // Read unread messages without marking them seen
    let unread = db
        .read_inbox("agent-reviewer", false, None)
        .expect("read unread messages");
    assert_eq!(unread.len(), 2);
    assert_eq!(unread[0].id, id1);
    assert_eq!(unread[0].ticket.as_deref(), Some("TASK-101"));
    assert_eq!(unread[0].frm, "agent-worker");
    assert_eq!(unread[0].too, "agent-reviewer");
    assert_eq!(unread[0].body, "Please review branch fix-issue");
    assert_eq!(unread[0].story.as_deref(), Some("story-alpha"));
    assert!(!unread[0].seen);
    assert!(!unread[0].at.is_empty());

    assert_eq!(unread[1].id, id2);
    assert_eq!(unread[1].ticket, None);
    assert_eq!(unread[1].frm, "agent-manager");
    assert_eq!(unread[1].too, "agent-reviewer");
    assert_eq!(unread[1].body, "Reminder about deadline");
    assert_eq!(unread[1].story, None);
    assert!(!unread[1].seen);

    // Read and mark them seen
    let consumed = db
        .read_inbox("agent-reviewer", true, None)
        .expect("read and mark seen");
    assert_eq!(consumed.len(), 2);
    assert!(consumed[0].seen);
    assert!(consumed[1].seen);

    // Subsequent inbox read should be empty
    let empty_inbox = db
        .read_inbox("agent-reviewer", false, None)
        .expect("read after consumption");
    assert!(empty_inbox.is_empty());
}

#[test]
fn test_mark_messages_seen_explicit() {
    let db = RigDatabase::open_in_memory().expect("open in memory db");

    let id1 = db
        .send_message(None, "dispatcher", "worker-1", "Job 1", None)
        .expect("send msg 1");
    let id2 = db
        .send_message(None, "dispatcher", "worker-1", "Job 2", None)
        .expect("send msg 2");
    let id3 = db
        .send_message(None, "dispatcher", "worker-1", "Job 3", None)
        .expect("send msg 3");

    // Empty slice should be a safe no-op
    db.mark_messages_seen(&[]).expect("mark empty slice");

    // Explicitly mark only id1 and id2 as seen
    db.mark_messages_seen(&[id1, id2]).expect("mark id1 and id2");

    // Only id3 should remain in inbox
    let remaining = db
        .read_inbox("worker-1", false, None)
        .expect("read remaining inbox");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, id3);
    assert_eq!(remaining[0].body, "Job 3");
    assert!(!remaining[0].seen);
}

#[test]
fn test_settings_persistence_and_defaults() {
    let temp_dir = tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    {
        let db = RigDatabase::open_repo(repo_root).expect("open repo db");

        // Verify default settings when key is not stored
        assert_eq!(
            db.get_setting_or_default("worker.harness")
                .expect("get worker.harness default"),
            "claude"
        );
        assert_eq!(
            db.get_setting_or_default("reviewer.harness")
                .expect("get reviewer.harness default"),
            "codex"
        );
        assert_eq!(
            db.get_setting_or_default("mode").expect("get mode default"),
            "pane"
        );
        assert_eq!(
            db.get_setting_or_default("worktree")
                .expect("get worktree default"),
            "on"
        );
        assert_eq!(
            db.get_setting_or_default("autonomy")
                .expect("get autonomy default"),
            "review"
        );
        assert_eq!(
            db.get_setting_or_default("merge").expect("get merge default"),
            "local"
        );
        assert_eq!(
            db.get_setting_or_default("custom.unknown")
                .expect("get unknown key"),
            ""
        );

        // get_setting returns None for unset keys
        assert_eq!(
            db.get_setting("worker.harness").expect("get unset setting"),
            None
        );

        // Set custom settings
        db.set_setting("worker.harness", "custom-worker")
            .expect("set worker.harness");
        db.set_setting("app.telemetry", "off")
            .expect("set app.telemetry");

        assert_eq!(
            db.get_setting("worker.harness").expect("get updated setting"),
            Some("custom-worker".to_string())
        );
        assert_eq!(
            db.get_setting_or_default("worker.harness")
                .expect("get updated default"),
            "custom-worker"
        );

        // Override existing setting
        db.set_setting("worker.harness", "anthropic-claude")
            .expect("override worker.harness");
        assert_eq!(
            db.get_setting("worker.harness").expect("get overriden setting"),
            Some("anthropic-claude".to_string())
        );

        let list = db.list_settings().expect("list settings");
        let mut expected = HashMap::new();
        expected.insert("worker.harness".to_string(), "anthropic-claude".to_string());
        expected.insert("app.telemetry".to_string(), "off".to_string());
        assert_eq!(list, expected);
    }

    // Verify persistence after reopening repository
    {
        let db = RigDatabase::open_repo(repo_root).expect("reopen repo db");
        assert_eq!(
            db.get_setting("worker.harness").expect("get persisted setting"),
            Some("anthropic-claude".to_string())
        );
        assert_eq!(
            db.get_setting("app.telemetry").expect("get persisted setting"),
            Some("off".to_string())
        );
    }

    // Direct helper function verification
    assert_eq!(default_setting("worker.harness"), "claude");
    assert_eq!(default_setting("reviewer.harness"), "codex");
    assert_eq!(default_setting("mode"), "pane");
    assert_eq!(default_setting("worktree"), "on");
    assert_eq!(default_setting("autonomy"), "review");
    assert_eq!(default_setting("merge"), "local");
    assert_eq!(default_setting("nonexistent"), "");
}

#[test]
fn test_message_story_filtering() {
    let db = RigDatabase::open_in_memory().expect("open in memory db");

    // Message without story (story IS NULL)
    db.send_message(None, "alice", "bob", "General announcement", None)
        .expect("send msg null story");

    // Message with story-1
    db.send_message(None, "alice", "bob", "Story 1 task", Some("story-1"))
        .expect("send msg story 1");

    // Message with story-2
    db.send_message(None, "alice", "bob", "Story 2 task", Some("story-2"))
        .expect("send msg story 2");

    // Filtering for story-1: should return messages with story == "story-1" OR story IS NULL
    let story1_inbox = db
        .read_inbox("bob", false, Some("story-1"))
        .expect("read inbox for story-1");
    assert_eq!(story1_inbox.len(), 2);
    assert_eq!(story1_inbox[0].body, "General announcement");
    assert_eq!(story1_inbox[1].body, "Story 1 task");

    // Filtering for story-2: should return messages with story == "story-2" OR story IS NULL
    let story2_inbox = db
        .read_inbox("bob", false, Some("story-2"))
        .expect("read inbox for story-2");
    assert_eq!(story2_inbox.len(), 2);
    assert_eq!(story2_inbox[0].body, "General announcement");
    assert_eq!(story2_inbox[1].body, "Story 2 task");

    // Unfiltered inbox: returns all 3 unread messages
    let all_inbox = db
        .read_inbox("bob", false, None)
        .expect("read all unread inbox");
    assert_eq!(all_inbox.len(), 3);
}

#[test]
fn test_send_message_params_struct() {
    let db = RigDatabase::open_in_memory().expect("open in memory db");

    let params = SendMessageParams {
        ticket: Some("TIC-42"),
        frm: "author",
        too: "receiver",
        body: "Payload body content",
        story: Some("story-main"),
    };

    let msg_id = db.send_message_params(&params).expect("send with params");
    assert!(msg_id > 0);

    let inbox = db
        .read_inbox("receiver", true, None)
        .expect("read receiver inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].id, msg_id);
    assert_eq!(inbox[0].ticket.as_deref(), Some("TIC-42"));
    assert_eq!(inbox[0].frm, "author");
    assert_eq!(inbox[0].too, "receiver");
    assert_eq!(inbox[0].body, "Payload body content");
    assert_eq!(inbox[0].story.as_deref(), Some("story-main"));
    assert!(inbox[0].seen);
}

#[test]
fn test_message_serialization_roundtrip() {
    let msg = Message {
        id: 99,
        ticket: Some("TIC-99".to_string()),
        frm: "worker-a".to_string(),
        too: "manager".to_string(),
        body: "Status update text".to_string(),
        seen: false,
        at: "2026-11-12T14:00:00Z".to_string(),
        story: Some("story-core".to_string()),
    };

    let json = serde_json::to_string(&msg).expect("serialize message");
    let deserialized: Message = serde_json::from_str(&json).expect("deserialize message");
    assert_eq!(deserialized, msg);
}

