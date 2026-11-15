//! Inter-agent Mailbox Module
//! Matches Go `tempterm/internal/mail/mail.go`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::registry::Registry;
use temp_terminal_proto::proto::{
    Mail, MailAskParams, MailReplyParams, MAIL_DELIVERED, MAIL_FAILED, MAIL_NO_REPLY, MAIL_QUEUED,
    MAIL_REPLIED,
};
use tokio::sync::broadcast;

pub const MAX_TEXT: usize = 64 * 1024; // 64 KiB
pub const KEPT: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Timeout(String),
    #[error("released")]
    Released,
    #[error("{0}")]
    Other(String),
}

pub struct Item {
    pub mail: Mail,
    pub waiters: usize,
    pub released: bool,
    pub left: Instant,
    pub told: bool,
    pub sending: bool,
    pub at: Instant,
    pub again: bool,
    pub worked: bool,
    pub taken: bool,
    pub past: bool,
    pub retyped: bool,
    pub noted: bool,
}

impl Item {
    pub fn settled(&self) -> bool {
        match self.mail.state.as_str() {
            MAIL_REPLIED | MAIL_NO_REPLY | MAIL_FAILED => true,
            _ => self.mail.note && self.mail.state == MAIL_DELIVERED && self.noted,
        }
    }

    pub fn open(&self) -> bool {
        self.mail.state == MAIL_QUEUED || self.mail.state == MAIL_DELIVERED
    }

    pub fn to_desc(&self) -> String {
        named(&self.mail.to_name, &self.mail.to)
    }
}

pub fn named(name: &str, pane: &str) -> String {
    if !name.is_empty() {
        name.to_string()
    } else {
        format!("pane {}", pane)
    }
}

fn check_text(text: &str, what: &str) -> Result<(), MailError> {
    if text.trim().is_empty() {
        return Err(MailError::Usage(format!("an empty {}", what)));
    }
    if text.len() > MAX_TEXT {
        return Err(MailError::Usage(format!(
            "a {} is at most 64 KiB: put the rest in a file, and name it",
            what
        )));
    }
    Ok(())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub struct Mailbox {
    reg: Arc<Registry>,
    next: AtomicI32,
    items: Mutex<Vec<Item>>,
    changed_tx: broadcast::Sender<()>,
    limits: Mutex<HashMap<(String, String), i32>>,
}

impl Mailbox {
    pub fn new(reg: Arc<Registry>) -> Arc<Self> {
        let (changed_tx, _) = broadcast::channel(128);
        Arc::new(Self {
            reg,
            next: AtomicI32::new(0),
            items: Mutex::new(Vec::new()),
            changed_tx,
            limits: Mutex::new(HashMap::new()),
        })
    }

    fn notify_changed(&self) {
        let _ = self.changed_tx.send(());
    }

    /// Ask queues a message for the agent in pane `p.to`.
    pub fn ask(&self, p: MailAskParams) -> Result<Mail, MailError> {
        check_text(&p.text, "message")?;
        if !p.from.is_empty() && p.from == p.to {
            return Err(MailError::Usage("that is your own pane".to_string()));
        }
        if self.reg.get(&p.to).is_none() {
            return Err(MailError::NotFound(format!("no pane {}", p.to)));
        }

        let mut items = self.items.lock().unwrap();
        if !p.note && !p.from.is_empty() {
            for it in items.iter() {
                if it.mail.from == p.to
                    && it.mail.to == p.from
                    && it.open()
                    && !it.mail.note
                    && it.waiters > 0
                {
                    return Err(MailError::Usage(format!(
                        "{} is waiting for your answer to {}: give it with tempterm reply {}",
                        named(&p.to_name, &p.to),
                        it.mail.id,
                        it.mail.id
                    )));
                }
            }
        }

        let mut limits = self.limits.lock().unwrap();
        let key = (p.from.clone(), p.to.clone());
        if let Some(n) = limits.get_mut(&key) {
            if !p.note {
                if *n <= 0 {
                    return Err(MailError::Usage(format!(
                        "you asked {} as many times as you may: tell the user what you found, with tempterm report done",
                        named(&p.to_name, &p.to)
                    )));
                }
                *n -= 1;
            }
        }
        drop(limits);

        let count = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let id = format!("m{}", count);
        let now = Instant::now();
        let mail = Mail {
            id,
            from: p.from,
            from_name: p.from_name,
            to: p.to,
            to_name: p.to_name,
            text: p.text,
            note: p.note,
            state: MAIL_QUEUED.to_string(),
            reply: String::new(),
            screen: String::new(),
            why: String::new(),
            created: now_millis(),
        };

        let it = Item {
            mail: mail.clone(),
            waiters: 0,
            released: p.no_wait || p.note,
            left: now,
            told: false,
            sending: false,
            at: now,
            again: false,
            worked: false,
            taken: false,
            past: false,
            retyped: false,
            noted: false,
        };

        items.push(it);
        self.trim_locked(&mut items);
        drop(items);

        self.notify_changed();
        Ok(mail)
    }

    /// Sets rate limit: pane `from` can ask pane `to` at most `n` times more.
    pub fn limit(&self, from: String, to: String, n: i32) {
        let mut limits = self.limits.lock().unwrap();
        limits.insert((from, to), n);
    }

    fn trim_locked(&self, items: &mut Vec<Item>) {
        let over = items.len().saturating_sub(KEPT);
        if over == 0 {
            return;
        }
        let mut to_drop = over;
        items.retain(|it| {
            if to_drop > 0
                && it.settled()
                && it.waiters == 0
                && (it.told || !it.released || it.mail.note)
            {
                to_drop -= 1;
                false
            } else {
                true
            }
        });
    }

    /// Awaits an answer to message `id`.
    pub async fn await_mail(
        &self,
        id: &str,
        timeout_opt: Option<Duration>,
    ) -> Result<Mail, MailError> {
        let mut rx = self.changed_tx.subscribe();
        {
            let mut items = self.items.lock().unwrap();
            let it = items.iter_mut().find(|i| i.mail.id == id);
            match it {
                Some(it) => {
                    it.waiters += 1;
                    it.released = false;
                }
                None => return Err(MailError::NotFound(format!("no message {}", id))),
            }
        }

        struct WaitGuard<'a> {
            id: String,
            mb: &'a Mailbox,
        }
        impl Drop for WaitGuard<'_> {
            fn drop(&mut self) {
                let mut items = self.mb.items.lock().unwrap();
                if let Some(it) = items.iter_mut().find(|i| i.mail.id == self.id) {
                    it.waiters = it.waiters.saturating_sub(1);
                    if it.waiters == 0 {
                        it.left = Instant::now();
                        it.released = it.released || !it.settled();
                    }
                }
            }
        }

        let _guard = WaitGuard {
            id: id.to_string(),
            mb: self,
        };

        let start = Instant::now();
        loop {
            {
                let items = self.items.lock().unwrap();
                if let Some(it) = items.iter().find(|i| i.mail.id == id) {
                    if it.settled() {
                        return Ok(it.mail.clone());
                    }
                    if it.released {
                        return Err(MailError::Released);
                    }
                } else {
                    return Err(MailError::NotFound(format!("no message {}", id)));
                }
            }

            if let Some(timeout) = timeout_opt {
                let elapsed = start.elapsed();
                if elapsed >= timeout {
                    let items = self.items.lock().unwrap();
                    let target_desc = items
                        .iter()
                        .find(|i| i.mail.id == id)
                        .map(|i| i.to_desc())
                        .unwrap_or_else(|| "target".to_string());
                    return Err(MailError::Timeout(format!(
                        "no answer from {} yet: it comes to you as a message once there is one",
                        target_desc
                    )));
                }
                let remaining = timeout - elapsed;
                tokio::select! {
                    _ = rx.recv() => {},
                    _ = tokio::time::sleep(remaining) => {},
                }
            } else {
                let _ = rx.recv().await;
            }
        }
    }

    /// Releases waiters for a message so answer goes as a message.
    pub fn release(&self, id: &str) {
        let mut items = self.items.lock().unwrap();
        if let Some(it) = items.iter_mut().find(|i| i.mail.id == id) {
            if !it.settled() {
                it.released = true;
                drop(items);
                self.notify_changed();
            }
        }
    }

    /// Answers a message.
    pub fn reply(&self, p: MailReplyParams) -> Result<Mail, MailError> {
        check_text(&p.text, "answer")?;
        let mut items = self.items.lock().unwrap();
        let target_idx = if p.id.is_empty() {
            let mut found = None;
            for (idx, x) in items.iter().enumerate().rev() {
                let answerable = x.mail.state == MAIL_DELIVERED
                    || x.mail.state == MAIL_NO_REPLY
                    || x.sending
                    || x.mail.state == MAIL_QUEUED;
                if x.mail.to == p.from && !x.mail.note && answerable {
                    found = Some(idx);
                    break;
                }
            }
            match found {
                Some(idx) => idx,
                None => {
                    return Err(MailError::NotFound(
                        "no message waits for your answer".to_string(),
                    ))
                }
            }
        } else {
            match items.iter().position(|i| i.mail.id == p.id) {
                Some(idx) => idx,
                None => return Err(MailError::NotFound(format!("no message {}", p.id))),
            }
        };

        {
            let it = &items[target_idx];
            if !p.from.is_empty() && it.mail.to != p.from {
                return Err(MailError::Usage(format!(
                    "{} was for {}, not for you",
                    it.mail.id,
                    it.to_desc()
                )));
            }
            if it.mail.note {
                return Err(MailError::Usage(format!("{} wants no answer", it.mail.id)));
            }
            if it.mail.state == MAIL_REPLIED {
                return Err(MailError::Usage(format!(
                    "{} has its answer already",
                    it.mail.id
                )));
            }
            if it.mail.state == MAIL_FAILED {
                return Err(MailError::Usage(format!("{} wasn't delivered", it.mail.id)));
            }
        }

        let it = &mut items[target_idx];
        if it.mail.state == MAIL_NO_REPLY && it.waiters == 0 {
            it.released = true;
            it.told = false;
        }
        it.mail.state = MAIL_REPLIED.to_string();
        it.mail.reply = p.text;
        let mail = it.mail.clone();
        drop(items);

        self.notify_changed();
        Ok(mail)
    }

    /// Lists messages to and from a pane, or all if empty.
    pub fn list(&self, pane: &str) -> Vec<Mail> {
        let items = self.items.lock().unwrap();
        items
            .iter()
            .filter(|it| pane.is_empty() || it.mail.from == pane || it.mail.to == pane)
            .map(|it| it.mail.clone())
            .collect()
    }
}
