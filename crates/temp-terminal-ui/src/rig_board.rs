//! Native Egui RigBoardPanel Widget.
//!
//! Provides the primary workspace inspection panel for Rig tickets,
//! grouping accordions, alert toast banners, and ticket interaction actions.

use std::path::PathBuf;

use eframe::egui::{self, Frame, Margin, Rounding, Stroke};
use temp_terminal_rig::{RigDatabase, Ticket, TicketFilter, TicketStatus};

use crate::workspace::theme;

/// State for the Rig Board UI panel.
#[derive(Debug, Clone)]
pub struct RigBoardState {
    pub repo_path: PathBuf,
    pub show_done: bool,
    pub expanded_groups: [bool; 6], // NeedsYou, Working, Ready, Queued, Todo, Done
    pub selected_ticket: Option<String>,
    pub pending_toasts: Vec<String>,
    pub action_feedback: Option<String>,
}

impl RigBoardState {
    /// Creates a new RigBoardState with default accordion states and filters.
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            repo_path,
            show_done: false,
            expanded_groups: [true, true, true, false, false, false],
            selected_ticket: None,
            pending_toasts: Vec::new(),
            action_feedback: None,
        }
    }

    /// Selects or deselects a ticket by ID.
    pub fn select_ticket(&mut self, id: Option<String>) {
        self.selected_ticket = id;
    }

    /// Toggles the expanded status of a group accordion.
    pub fn toggle_group(&mut self, idx: usize) {
        if idx < self.expanded_groups.len() {
            self.expanded_groups[idx] = !self.expanded_groups[idx];
        }
    }

    /// Checks the database for tickets requiring user attention and appends toast alerts.
    /// Avoids duplicate toast messages for the same ticket.
    pub fn check_alerts(&mut self, db: &mut RigDatabase) {
        let filter = TicketFilter {
            include_done: false,
            ..Default::default()
        };

        if let Ok(tickets) = db.list_tickets(&filter) {
            for t in tickets {
                if t.status == TicketStatus::NeedsYou || t.status == TicketStatus::Blocked {
                    let alert = format!("{}: {} [{}]", t.id, t.title, t.status);
                    if !self.pending_toasts.contains(&alert) {
                        self.pending_toasts.push(alert);
                    }
                }
            }
        }
    }

    /// Pops the oldest pending toast alert, if any.
    pub fn pop_toast(&mut self) -> Option<String> {
        if self.pending_toasts.is_empty() {
            None
        } else {
            Some(self.pending_toasts.remove(0))
        }
    }
}

/// Native Egui Rig Board Panel.
pub struct RigBoardPanel;

impl RigBoardPanel {
    /// Renders the complete Rig Board panel UI inside the provided Egui Ui context.
    pub fn ui(ui: &mut egui::Ui, state: &mut RigBoardState, db: &mut RigDatabase) {
        // 1. Toast Notification Banner
        let mut should_pop_toast = false;
        if !state.pending_toasts.is_empty() {
            let toast_frame = Frame::none()
                .fill(theme::SURFACE_ELEVATED)
                .stroke(Stroke::new(1.0_f32, theme::STATUS_WARNING))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(6.0));

            toast_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("ALERT:")
                            .strong()
                            .size(11.0)
                            .color(theme::STATUS_WARNING),
                    );
                    if let Some(msg) = state.pending_toasts.first() {
                        ui.label(
                            egui::RichText::new(msg)
                                .size(11.0)
                                .color(theme::TEXT_PRIMARY),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui::RichText::new("Dismiss").size(10.0).color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            should_pop_toast = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        if should_pop_toast {
            state.pop_toast();
        }

        // 2. Action Feedback Banner
        let mut clear_feedback = false;
        if let Some(ref feedback) = state.action_feedback {
            let feedback_frame = Frame::none()
                .fill(theme::SURFACE_CARD)
                .stroke(Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(6.0));

            feedback_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(feedback)
                            .size(11.0)
                            .color(theme::STATUS_OK),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui::RichText::new("x").size(10.0).color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            clear_feedback = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        if clear_feedback {
            state.action_feedback = None;
        }

        // 3. Controls Bar
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("RIG TICKETS")
                    .size(10.0)
                    .color(theme::TEXT_MUTED)
                    .extra_letter_spacing(1.0),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut state.show_done, "Show Done");
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // 4. Retrieve Tickets
        let filter = TicketFilter {
            include_done: state.show_done,
            ..Default::default()
        };
        let tickets = db.list_tickets(&filter).unwrap_or_default();

        let mut groups: [Vec<Ticket>; 6] = [
            Vec::new(), // 0: Needs you
            Vec::new(), // 1: Working
            Vec::new(), // 2: Ready
            Vec::new(), // 3: Queued
            Vec::new(), // 4: To do
            Vec::new(), // 5: Done
        ];

        for t in tickets {
            let group_name = db.group_of(&t).unwrap_or(match t.status {
                TicketStatus::NeedsYou | TicketStatus::Blocked => "Needs you",
                TicketStatus::Working | TicketStatus::Reviewing => "Working",
                TicketStatus::Review => "Ready",
                TicketStatus::Done => "Done",
                TicketStatus::Draft => "Queued",
                TicketStatus::Open => "To do",
            });
            match group_name {
                "Needs you" => groups[0].push(t),
                "Working" => groups[1].push(t),
                "Ready" => groups[2].push(t),
                "Queued" => groups[3].push(t),
                "To do" => groups[4].push(t),
                "Done" => groups[5].push(t),
                _ => groups[4].push(t),
            }
        }

        let group_meta = [
            ("Needs you", theme::STATUS_ERROR),
            ("Working", theme::STATUS_WARNING),
            ("Ready", theme::STATUS_OK),
            ("Queued", theme::TEXT_MUTED),
            ("To do", theme::TEXT_FAINT),
            ("Done", theme::TEXT_MUTED),
        ];

        // 5. Accordion Groups
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, (name, color)) in group_meta.iter().enumerate() {
                // If Done is not requested to be shown, skip rendering Done group
                if idx == 5 && !state.show_done {
                    continue;
                }

                let is_expanded = state.expanded_groups[idx];
                let count = groups[idx].len();

                // Accordion Header
                let header_text = format!(
                    "{} {} ({})",
                    if is_expanded { "[-] " } else { "[+] " },
                    name,
                    count
                );

                let header_btn = egui::Button::new(
                    egui::RichText::new(header_text)
                        .strong()
                        .size(11.5)
                        .color(*color),
                )
                .fill(egui::Color32::TRANSPARENT)
                .stroke(Stroke::NONE);

                if ui.add(header_btn).clicked() {
                    state.toggle_group(idx);
                }

                if is_expanded {
                    ui.indent(format!("rig_group_{}", idx), |ui| {
                        if groups[idx].is_empty() {
                            ui.label(
                                egui::RichText::new("No tickets")
                                    .size(11.0)
                                    .color(theme::TEXT_FAINT),
                            );
                        } else {
                            for t in &groups[idx] {
                                Self::render_ticket_card(ui, state, t);
                                ui.add_space(4.0);
                            }
                        }
                    });
                }

                ui.add_space(4.0);
            }
        });
    }

    /// Renders an individual ticket card following strict design rules.
    fn render_ticket_card(ui: &mut egui::Ui, state: &mut RigBoardState, t: &Ticket) {
        let is_selected = state.selected_ticket.as_deref() == Some(&t.id);

        let stroke = if is_selected {
            Stroke::new(1.5_f32, theme::ACCENT)
        } else {
            Stroke::new(1.0_f32, theme::BORDER_SUBTLE)
        };

        let fill = if is_selected {
            theme::SURFACE_ELEVATED
        } else {
            theme::SURFACE_CARD
        };

        let card_frame = Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(8.0));

        let mut card_clicked = false;
        let mut clicked_action: Option<String> = None;

        let resp = card_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // Header line: ID, Status, Branch badge, Manager
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&t.id)
                        .strong()
                        .size(12.0)
                        .color(theme::TEXT_PRIMARY),
                );

                let status_color = match t.status {
                    TicketStatus::NeedsYou | TicketStatus::Blocked => theme::STATUS_ERROR,
                    TicketStatus::Working | TicketStatus::Reviewing => theme::STATUS_WARNING,
                    TicketStatus::Review => theme::STATUS_OK,
                    TicketStatus::Done => theme::TEXT_MUTED,
                    _ => theme::TEXT_FAINT,
                };

                ui.label(
                    egui::RichText::new(format!("[{}]", t.status))
                        .size(10.0)
                        .color(status_color),
                );

                if let Some(ref branch) = t.branch {
                    ui.label(
                        egui::RichText::new(branch)
                            .size(10.0)
                            .color(theme::TEXT_MUTED),
                    );
                } else {
                    let default_branch = format!("branch/rig/{}", t.id.to_lowercase());
                    ui.label(
                        egui::RichText::new(default_branch)
                            .size(10.0)
                            .color(theme::TEXT_MUTED),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("mgr: {}", t.manager))
                            .size(10.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
            });

            ui.add_space(2.0);

            // Ticket Title
            ui.label(
                egui::RichText::new(&t.title)
                    .size(11.5)
                    .color(theme::TEXT_NORMAL),
            );

            // Action Buttons
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let action_btn = |ui: &mut egui::Ui, label: &str| -> bool {
                    ui.add(
                        egui::Button::new(
                            egui::RichText::new(label)
                                .size(10.0)
                                .color(theme::TEXT_MUTED),
                        )
                        .fill(theme::SURFACE_CARD)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
                        .rounding(Rounding::same(3.0)),
                    )
                    .clicked()
                };

                if action_btn(ui, "Answer") {
                    clicked_action = Some(format!("Action: Answering ticket {}", t.id));
                }
                if action_btn(ui, "Review") {
                    clicked_action = Some(format!("Action: Reviewing ticket {}", t.id));
                }
                if action_btn(ui, "Steer") {
                    clicked_action = Some(format!("Action: Steering ticket {}", t.id));
                }
                if action_btn(ui, "Open Pane") {
                    clicked_action = Some(format!("Action: Opening pane for ticket {}", t.id));
                }
            });
        });

        if resp.response.clicked() {
            card_clicked = true;
        }

        if card_clicked {
            state.select_ticket(Some(t.id.clone()));
        }

        if let Some(act) = clicked_action {
            state.action_feedback = Some(act);
            state.select_ticket(Some(t.id.clone()));
        }
    }
}
