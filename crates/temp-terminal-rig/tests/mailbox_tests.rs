use std::thread;
use std::time::Duration;
use temp_terminal_rig::{InboxWaitOptions, MailboxListener, RigDatabase};

#[test]
fn test_mailbox_wait_immediate_messages() {
    let mut db = RigDatabase::open_in_memory().expect("open memory db");

    db.send_message(Some("T-1"), "sender", "manager", "Hello immediately", Some("s-1"))
        .expect("send message");

    let options = InboxWaitOptions {
        recipient: "manager".to_string(),
        ticket_filter: None,
        story_filter: None,
        timeout_ms: Some(2000),
        poll_interval_ms: 50,
    };

    let start = std::time::Instant::now();
    let messages = MailboxListener::wait_for_messages(&mut db, &options).expect("wait for messages");
    let elapsed = start.elapsed();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "Hello immediately");
    assert!(messages[0].seen); // Should be marked seen automatically
    assert!(elapsed < Duration::from_millis(500)); // Should return immediately without sleeping

    // Subsequent read should find inbox empty (consumed and marked seen)
    let unread = db.read_inbox("manager", false, None).expect("read inbox");
    assert!(unread.is_empty());
}

#[test]
fn test_mailbox_wait_blocking_until_message_arrives() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();

    let mut db1 = RigDatabase::open_repo(repo_root).expect("open db1");
    let repo_root_clone = repo_root.to_path_buf();

    // Spawn a thread to send a message after 150ms
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        let db2 = RigDatabase::open_repo(&repo_root_clone).expect("open db2");
        db2.send_message(None, "worker", "manager", "Delayed greeting", None)
            .expect("send delayed message");
    });

    let options = InboxWaitOptions {
        recipient: "manager".to_string(),
        ticket_filter: None,
        story_filter: None,
        timeout_ms: Some(3000),
        poll_interval_ms: 30,
    };

    let messages = MailboxListener::wait_for_messages(&mut db1, &options).expect("wait for messages");
    handle.join().expect("join thread");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "Delayed greeting");
    assert!(messages[0].seen);
}

#[test]
fn test_mailbox_wait_timeout() {
    let mut db = RigDatabase::open_in_memory().expect("open memory db");

    let options = InboxWaitOptions {
        recipient: "manager".to_string(),
        ticket_filter: None,
        story_filter: None,
        timeout_ms: Some(150), // Short timeout
        poll_interval_ms: 30,
    };

    let start = std::time::Instant::now();
    let messages = MailboxListener::wait_for_messages(&mut db, &options).expect("wait for messages");
    let elapsed = start.elapsed();

    assert!(messages.is_empty());
    assert!(elapsed >= Duration::from_millis(100));
}

#[test]
fn test_mailbox_wait_filters() {
    let mut db = RigDatabase::open_in_memory().expect("open memory db");

    // Send messages with different tickets and stories
    db.send_message(Some("T-1"), "alice", "manager", "Msg 1", Some("story-a"))
        .expect("send 1");
    db.send_message(Some("T-2"), "bob", "manager", "Msg 2", Some("story-b"))
        .expect("send 2");

    // Filter by ticket T-2
    let options = InboxWaitOptions {
        recipient: "manager".to_string(),
        ticket_filter: Some("T-2".to_string()),
        story_filter: None,
        timeout_ms: Some(1000),
        poll_interval_ms: 50,
    };

    let messages = MailboxListener::wait_for_messages(&mut db, &options).expect("wait");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "Msg 2");

    // Msg 1 should still be unread in inbox
    let remaining = db.read_inbox("manager", false, None).expect("remaining");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].body, "Msg 1");
}
