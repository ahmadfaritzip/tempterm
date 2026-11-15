use serde::{Serialize, Deserialize};
use crate::db::RigDatabase;
use crate::error::RigError;
use crate::ticket::TicketFilter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardRenderOptions {
    pub show_done: bool,
    pub show_activity: bool,
    pub max_activity_items: usize,
    pub spinner_tick: usize,
}

impl Default for BoardRenderOptions {
    fn default() -> Self {
        Self {
            show_done: false,
            show_activity: true,
            max_activity_items: 10,
            spinner_tick: 0,
        }
    }
}

pub struct BoardTuiRenderer;

impl BoardTuiRenderer {
    pub const SPINNER_FRAMES: [&'static str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    pub fn render_effort_gauge(effort: u32) -> &'static str {
        match effort {
            1 => "❚❘❘❘",
            2 => "❚❚❘❘",
            3 => "❚❚❚❘",
            _ => "❚❚❚❚",
        }
    }

    pub fn render_board(db: &mut RigDatabase, options: &BoardRenderOptions) -> Result<String, RigError> {
        let mut filter = TicketFilter::default();
        if options.show_done {
            filter.include_done = true;
        }
        let tickets = db.list_tickets(&filter)?;

        let mut needs_you = Vec::new();
        let mut working = Vec::new();
        let mut ready = Vec::new();
        let mut queued = Vec::new();
        let mut todo = Vec::new();
        let mut done = Vec::new();

        for t in tickets {
            let group = db.group_of(&t)?;
            match group {
                "Needs you" => needs_you.push(t),
                "Working" => working.push(t),
                "Ready" => ready.push(t),
                "Queued" => queued.push(t),
                "To do" => todo.push(t),
                "Done" => done.push(t),
                _ => todo.push(t),
            }
        }

        let mut output = String::new();
        output.push_str("=== RIG BOARD TUI ===\n");

        let spinner = Self::SPINNER_FRAMES[options.spinner_tick % Self::SPINNER_FRAMES.len()];

        let groups = vec![
            ("Needs you", "\x1b[31m", "●", &needs_you, true),
            ("Working", "\x1b[33m", spinner, &working, true),
            ("Ready", "\x1b[32m", "◆", &ready, true),
            ("Queued", "\x1b[90m", "◷", &queued, true),
            ("To do", "\x1b[37m", "○", &todo, true),
        ];

        for (title, color, symbol, list, always_show) in groups {
            if !always_show && list.is_empty() {
                continue;
            }
            output.push_str(&format!("{}{} [{}] ({})\x1b[0m\n", color, symbol, title, list.len()));
            for t in list {
                let effort = 3;
                let gauge = Self::render_effort_gauge(effort);
                let branch_str = t.branch.clone().unwrap_or_else(|| format!("rig/{}", t.id));
                let harness_str = t.manager.clone();
                output.push_str(&format!(
                    "  [{}] {} | ⎇ {} | {} | effort: {}\n",
                    t.id, t.title, branch_str, harness_str, gauge
                ));
            }
        }

        if options.show_done {
            output.push_str(&format!("\x1b[90m✓ [Done] ({})\x1b[0m\n", done.len()));
            for t in done {
                let effort = 3;
                let gauge = Self::render_effort_gauge(effort);
                let branch_str = t.branch.clone().unwrap_or_else(|| format!("rig/{}", t.id));
                let harness_str = t.manager.clone();
                output.push_str(&format!(
                    "  [{}] {} | ⎇ {} | {} | effort: {}\n",
                    t.id, t.title, branch_str, harness_str, gauge
                ));
            }
        }

        if options.show_activity {
            output.push_str("\n--- Activity Stream ---\n");
            if let Ok(messages) = db.read_inbox("manager", false, None) {
                let limit = options.max_activity_items;
                let count = messages.len();
                let start = if count > limit { count - limit } else { 0 };
                for msg in &messages[start..] {
                    output.push_str(&format!("12:00 · [{}] {} -> {}: {}\n", msg.ticket.as_deref().unwrap_or("T-?"), msg.frm, msg.too, msg.body));
                }
            }
        }

        Ok(output)
    }
}
