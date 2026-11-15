use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::ticket::TicketStatus;
use temp_terminal_rig::tui::{BoardRenderOptions, BoardTuiRenderer};

#[test]
fn test_render_effort_gauge() {
    assert_eq!(BoardTuiRenderer::render_effort_gauge(1), "❚❘❘❘");
    assert_eq!(BoardTuiRenderer::render_effort_gauge(2), "❚❚❘❘");
    assert_eq!(BoardTuiRenderer::render_effort_gauge(3), "❚❚❚❘");
    assert_eq!(BoardTuiRenderer::render_effort_gauge(4), "❚❚❚❚");
    assert_eq!(BoardTuiRenderer::render_effort_gauge(5), "❚❚❚❚");
}

#[test]
fn test_board_tui_renderer_groups_and_formatting() {
    let mut db = RigDatabase::open_in_memory().unwrap();

    let t1 = db.create_ticket("Needs You Ticket", "Needs attention", "claude", false, None, None, None).unwrap();
    db.update_ticket_status(&t1.id, TicketStatus::NeedsYou).unwrap();

    let t2 = db.create_ticket("Working Ticket", "Active work", "claude", false, None, None, None).unwrap();
    db.update_ticket_status(&t2.id, TicketStatus::Working).unwrap();

    let t3 = db.create_ticket("Ready Ticket", "Ready for review", "claude", true, None, None, None).unwrap();
    db.update_ticket_status(&t3.id, TicketStatus::Review).unwrap();

    let mut options = BoardRenderOptions::default();
    options.show_activity = true;
    options.show_done = true;
    options.spinner_tick = 2; // SPINNER_FRAMES[2] = "⠹"

    let rendered = BoardTuiRenderer::render_board(&mut db, &options).unwrap();
    eprintln!("RENDERED:\n{}", rendered);

    assert!(rendered.contains("\x1b[31m")); // Needs you red
    assert!(rendered.contains("\x1b[33m")); // Working amber/yellow
    assert!(rendered.contains("\x1b[32m")); // Ready green
    assert!(rendered.contains("⠹"));       // Spinner frame at tick 2
    assert!(rendered.contains("rig/T-1"));
    assert!(rendered.contains("❚❚❚❘"));     // Effort gauge
}
