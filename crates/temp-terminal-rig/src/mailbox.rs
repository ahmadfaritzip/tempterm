use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::{RigDatabase, RigError, Message};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxWaitOptions {
    pub recipient: String,
    pub ticket_filter: Option<String>,
    pub story_filter: Option<String>,
    pub timeout_ms: Option<u64>,
    pub poll_interval_ms: u64,
}

impl Default for InboxWaitOptions {
    fn default() -> Self {
        Self {
            recipient: "manager".to_string(),
            ticket_filter: None,
            story_filter: None,
            timeout_ms: Some(10_000),
            poll_interval_ms: 250,
        }
    }
}

pub struct MailboxListener;

impl MailboxListener {
    pub fn wait_for_messages(
        db: &mut RigDatabase,
        options: &InboxWaitOptions,
    ) -> Result<Vec<Message>, RigError> {
        let start_time = std::time::Instant::now();
        let timeout = options.timeout_ms.map(Duration::from_millis);

        loop {
            // Read unread messages without marking them seen yet
            let raw_messages = db.read_inbox(&options.recipient, false, options.story_filter.as_deref())?;

            // Filter by ticket if provided
            let filtered: Vec<Message> = raw_messages
                .into_iter()
                .filter(|msg| {
                    if let Some(ref filter_ticket) = options.ticket_filter {
                        match &msg.ticket {
                            Some(t) => t == filter_ticket,
                            None => false,
                        }
                    } else {
                        true
                    }
                })
                .collect();

            if !filtered.is_empty() {
                let ids: Vec<i64> = filtered.iter().map(|m| m.id).collect();
                db.mark_messages_seen(&ids)?;
                // Re-read or populate seen = true on returned messages
                let mut marked = filtered;
                for m in &mut marked {
                    m.seen = true;
                }
                return Ok(marked);
            }

            // Check timeout
            if let Some(dur) = timeout {
                if start_time.elapsed() >= dur {
                    return Ok(vec![]);
                }
            }

            std::thread::sleep(Duration::from_millis(options.poll_interval_ms));
        }
    }
}
