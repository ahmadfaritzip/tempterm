//! Collection and lifecycle tracking of pane OSC status records.
//! Ported with exact parity from `tempterm/internal/osc/records.go`.

use std::collections::HashMap;
use temp_terminal_proto::proto::OscRecord;

/// Maximum number of status records retained per pane.
pub const MAX_RECORDS: usize = 64;

/// Sub-id used for agent status reports.
pub const AGENT_ID: &str = "agent";

/// Pane status report repository keyed by record ID.
#[derive(Debug, Clone, Default)]
pub struct Records {
    m: HashMap<String, OscRecord>,
}

impl Records {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a record update, clear, or replacement.
    pub fn apply(&mut self, r: OscRecord) {
        if r.state == "clear" {
            if r.id.is_empty() {
                self.m.clear();
                return;
            }
            let prefix = format!("{}/", r.id);
            self.m.retain(|k, _| k != &r.id && !k.starts_with(&prefix));
            return;
        }

        if r.id.is_empty() && (r.state == "done" || r.state == "idle") {
            self.end_turn(r.updated);
        }

        if !self.m.contains_key(&r.id) && self.m.len() >= MAX_RECORDS {
            if let Some(oldest_key) = self
                .m
                .iter()
                .min_by_key(|(_, v)| v.updated)
                .map(|(k, _)| k.clone())
            {
                self.m.remove(&oldest_key);
            }
        }

        self.m.insert(r.id.clone(), r);
    }

    pub fn reset(&mut self) {
        self.m.clear();
    }

    pub fn len(&self) -> usize {
        self.m.len()
    }

    pub fn is_empty(&self) -> bool {
        self.m.is_empty()
    }

    pub fn drop_live(&mut self) -> bool {
        self.drop_states(&["working", "blocked", "idle"])
    }

    pub fn dismiss_finished(&mut self) -> bool {
        self.drop_states(&["done", "error"])
    }

    fn drop_states(&mut self, states: &[&str]) -> bool {
        let initial_len = self.m.len();
        self.m.retain(|_, v| !states.contains(&v.state.as_str()));
        self.m.len() != initial_len
    }

    pub fn end_turn(&mut self, at: i64) -> bool {
        let mut changed = false;
        let prefix = format!("{}/", AGENT_ID);
        for (id, r) in self.m.iter_mut() {
            if r.state == "working" && (id == AGENT_ID || id.starts_with(&prefix)) {
                r.state = "done".to_string();
                r.progress = -1;
                r.updated = at;
                changed = true;
            }
        }
        changed
    }

    pub fn agent_working(&self) -> bool {
        let prefix = format!("{}/", AGENT_ID);
        self.m
            .iter()
            .any(|(id, r)| r.state == "working" && (id == AGENT_ID || id.starts_with(&prefix)))
    }

    pub fn declared(&self) -> bool {
        self.m
            .values()
            .any(|r| r.state == "working" || r.state == "blocked")
    }

    pub fn list(&self) -> Vec<OscRecord> {
        let mut recs: Vec<OscRecord> = self.m.values().cloned().collect();
        recs.sort_by(|a, b| a.id.cmp(&b.id));

        let mut apps: HashMap<String, String> = HashMap::new();
        for r in &recs {
            apps.insert(r.id.clone(), r.app.clone());
        }

        for r in &mut recs {
            let mut curr = r.id.clone();
            while r.app.is_empty() && !curr.is_empty() {
                if let Some(j) = curr.rfind('/') {
                    curr.truncate(j);
                } else {
                    curr.clear();
                }
                if let Some(parent_app) = apps.get(&curr) {
                    if !parent_app.is_empty() {
                        r.app = parent_app.clone();
                        break;
                    }
                }
            }
        }

        recs
    }

    pub fn lead(&self) -> Option<OscRecord> {
        let recs = self.list();
        for state in &["blocked", "working", "error", "done", "idle"] {
            for worded in &[true, false] {
                for r in &recs {
                    if r.state == *state && (!r.msg.is_empty() || !*worded) {
                        return Some(r.clone());
                    }
                }
            }
        }
        None
    }
}
