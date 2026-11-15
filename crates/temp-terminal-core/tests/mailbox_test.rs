//! Tests for Mailbox core logic: ask, reply, await, timeout, limits, and listing.
//! Corresponds to Go `tempterm/internal/mail/mail_test.go`.

use std::sync::Arc;
use std::time::Duration;

use temp_terminal_core::mail::{MailError, Mailbox};
use temp_terminal_core::registry::{Registry, StartPaneOptions};
use temp_terminal_proto::proto::{MailAskParams, MailReplyParams, MAIL_QUEUED, MAIL_REPLIED};

fn setup_mailbox_with_panes(pane_ids: &[&str]) -> (Arc<Registry>, Arc<Mailbox>) {
    let reg = Arc::new(Registry::new());
    for &id in pane_ids {
        #[cfg(windows)]
        let cmd = vec![
            "cmd.exe".to_string(),
            "/c".to_string(),
            "exit 0".to_string(),
        ];
        #[cfg(not(windows))]
        let cmd = vec!["sh".to_string(), "-c".to_string(), "exit 0".to_string()];

        let _ = reg.start(StartPaneOptions {
            id: id.to_string(),
            session: "sess_1".to_string(),
            project: "proj_1".to_string(),
            role: "agent".to_string(),
            dir: String::new(),
            command: cmd,
            env: Vec::new(),
            cols: 80,
            rows: 24,
        });
    }
    let mb = Mailbox::new(reg.clone());
    (reg, mb)
}

#[tokio::test]
async fn test_mailbox_ask_validation_and_not_found() {
    let (_reg, mb) = setup_mailbox_with_panes(&["pane_lead", "pane_worker"]);

    // Empty message
    let err = mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "pane_worker".to_string(),
            text: "   ".to_string(),
            ..Default::default()
        })
        .unwrap_err();
    assert!(matches!(err, MailError::Usage(_)));

    // Asking own pane
    let err = mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "pane_lead".to_string(),
            text: "hello myself".to_string(),
            ..Default::default()
        })
        .unwrap_err();
    assert!(matches!(err, MailError::Usage(_)));

    // Target not found
    let err = mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "nonexistent".to_string(),
            text: "hello".to_string(),
            ..Default::default()
        })
        .unwrap_err();
    assert!(matches!(err, MailError::NotFound(_)));
}

#[tokio::test]
async fn test_mailbox_ask_reply_await_flow() {
    let (_reg, mb) = setup_mailbox_with_panes(&["pane_lead", "pane_worker"]);

    let mail = mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            from_name: "lead".to_string(),
            to: "pane_worker".to_string(),
            to_name: "worker".to_string(),
            text: "Fix bug 42".to_string(),
            ..Default::default()
        })
        .expect("ask succeeds");

    assert_eq!(mail.id, "m1");
    assert_eq!(mail.state, MAIL_QUEUED);
    assert_eq!(mail.text, "Fix bug 42");

    let mb_clone = mb.clone();
    let msg_id = mail.id.clone();

    // Spawn async task answering the message after short delay
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        mb_clone
            .reply(MailReplyParams {
                id: msg_id,
                from: "pane_worker".to_string(),
                text: "Bug 42 is fixed".to_string(),
            })
            .expect("reply succeeds");
    });

    // Await message response
    let answered = mb
        .await_mail(&mail.id, Some(Duration::from_secs(2)))
        .await
        .expect("await succeeds");

    assert_eq!(answered.id, "m1");
    assert_eq!(answered.state, MAIL_REPLIED);
    assert_eq!(answered.reply, "Bug 42 is fixed");
}

#[tokio::test]
async fn test_mailbox_await_timeout() {
    let (_reg, mb) = setup_mailbox_with_panes(&["pane_lead", "pane_worker"]);

    let mail = mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "pane_worker".to_string(),
            to_name: "w1".to_string(),
            text: "Takes too long".to_string(),
            ..Default::default()
        })
        .expect("ask succeeds");

    let err = mb
        .await_mail(&mail.id, Some(Duration::from_millis(100)))
        .await
        .unwrap_err();

    match err {
        MailError::Timeout(msg) => {
            assert!(msg.contains("no answer from w1 yet"));
        }
        other => panic!("expected timeout error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_mailbox_rate_limit() {
    let (_reg, mb) = setup_mailbox_with_panes(&["pane_lead", "pane_worker"]);

    // Limit to 2 rounds
    mb.limit("pane_lead".to_string(), "pane_worker".to_string(), 2);

    // Ask 1
    assert!(mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "pane_worker".to_string(),
            text: "q1".to_string(),
            ..Default::default()
        })
        .is_ok());

    // Ask 2
    assert!(mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "pane_worker".to_string(),
            text: "q2".to_string(),
            ..Default::default()
        })
        .is_ok());

    // Ask 3 -> limit exceeded
    let err = mb
        .ask(MailAskParams {
            from: "pane_lead".to_string(),
            to: "pane_worker".to_string(),
            text: "q3".to_string(),
            ..Default::default()
        })
        .unwrap_err();

    assert!(matches!(err, MailError::Usage(_)));
}

#[tokio::test]
async fn test_mailbox_list_and_filter() {
    let (_reg, mb) = setup_mailbox_with_panes(&["p1", "p2", "p3"]);

    mb.ask(MailAskParams {
        from: "p1".to_string(),
        to: "p2".to_string(),
        text: "hello p2".to_string(),
        ..Default::default()
    })
    .unwrap();

    mb.ask(MailAskParams {
        from: "p1".to_string(),
        to: "p3".to_string(),
        text: "hello p3".to_string(),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(mb.list("").len(), 2);
    assert_eq!(mb.list("p2").len(), 1);
    assert_eq!(mb.list("p3").len(), 1);
    assert_eq!(mb.list("p1").len(), 2);
}
