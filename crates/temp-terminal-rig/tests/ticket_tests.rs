use temp_terminal_rig::{RigDatabase, RigError, TicketFilter, TicketStatus};

#[test]
fn test_ticket_status_parsing_and_display() {
    assert_eq!("open".parse::<TicketStatus>().unwrap(), TicketStatus::Open);
    assert_eq!("draft".parse::<TicketStatus>().unwrap(), TicketStatus::Draft);
    assert_eq!("working".parse::<TicketStatus>().unwrap(), TicketStatus::Working);
    assert_eq!("reviewing".parse::<TicketStatus>().unwrap(), TicketStatus::Reviewing);
    assert_eq!("review".parse::<TicketStatus>().unwrap(), TicketStatus::Review);
    assert_eq!("needs_you".parse::<TicketStatus>().unwrap(), TicketStatus::NeedsYou);
    assert_eq!("needs-you".parse::<TicketStatus>().unwrap(), TicketStatus::NeedsYou);
    assert_eq!("needs you".parse::<TicketStatus>().unwrap(), TicketStatus::NeedsYou);
    assert_eq!("blocked".parse::<TicketStatus>().unwrap(), TicketStatus::Blocked);
    assert_eq!("done".parse::<TicketStatus>().unwrap(), TicketStatus::Done);

    assert_eq!(TicketStatus::Open.as_str(), "open");
    assert_eq!(TicketStatus::Draft.as_str(), "draft");
    assert_eq!(TicketStatus::Working.as_str(), "working");
    assert_eq!(TicketStatus::Reviewing.as_str(), "reviewing");
    assert_eq!(TicketStatus::Review.as_str(), "review");
    assert_eq!(TicketStatus::NeedsYou.as_str(), "needs_you");
    assert_eq!(TicketStatus::Blocked.as_str(), "blocked");
    assert_eq!(TicketStatus::Done.as_str(), "done");

    assert!("unknown_status".parse::<TicketStatus>().is_err());
}

#[test]
fn test_ticket_status_serde_roundtrip() {
    let json = serde_json::to_string(&TicketStatus::NeedsYou).unwrap();
    assert_eq!(json, "\"needs_you\"");
    let status: TicketStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(status, TicketStatus::NeedsYou);
}

#[test]
fn test_atomic_auto_increment_id_generation() {
    let db = RigDatabase::open_in_memory().unwrap();

    let t1 = db
        .create_ticket("Ticket One", "Body 1", "lead", false, None, None, None)
        .unwrap();
    assert_eq!(t1.id, "T-1");

    let t2 = db
        .create_ticket("Ticket Two", "Body 2", "lead", true, None, None, None)
        .unwrap();
    assert_eq!(t2.id, "T-2");

    let t3 = db
        .create_ticket("Ticket Three", "Body 3", "worker", false, None, None, None)
        .unwrap();
    assert_eq!(t3.id, "T-3");
}

#[test]
fn test_create_and_get_ticket() {
    let db = RigDatabase::open_in_memory().unwrap();

    let created = db
        .create_ticket(
            "Feature Login",
            "Implement OAuth2 flow",
            "security-lead",
            true,
            Some("T-0"),
            Some("main"),
            Some("STORY-42"),
        )
        .unwrap();

    assert_eq!(created.id, "T-1");
    assert_eq!(created.title, "Feature Login");
    assert_eq!(created.body, "Implement OAuth2 flow");
    assert_eq!(created.status, TicketStatus::Open);
    assert_eq!(created.manager, "security-lead");
    assert!(created.review);
    assert_eq!(created.after.as_deref(), Some("T-0"));
    assert_eq!(created.base.as_deref(), Some("main"));
    assert_eq!(created.story.as_deref(), Some("STORY-42"));
    assert!(!created.created.is_empty());

    let fetched = db.get_ticket("T-1").unwrap();
    assert_eq!(fetched, created);

    let not_found = db.get_ticket("T-999");
    assert!(matches!(not_found, Err(RigError::TicketNotFound(id)) if id == "T-999"));
}

#[test]
fn test_update_ticket_status() {
    let db = RigDatabase::open_in_memory().unwrap();

    let ticket = db
        .create_ticket("Task", "Description", "lead", false, None, None, None)
        .unwrap();

    db.update_ticket_status(&ticket.id, TicketStatus::Working).unwrap();
    let updated = db.get_ticket(&ticket.id).unwrap();
    assert_eq!(updated.status, TicketStatus::Working);

    let err = db.update_ticket_status("T-999", TicketStatus::Done);
    assert!(matches!(err, Err(RigError::TicketNotFound(id)) if id == "T-999"));
}

#[test]
fn test_update_ticket_fields() {
    let db = RigDatabase::open_in_memory().unwrap();

    let mut ticket = db
        .create_ticket("Old Title", "Old Body", "lead", false, None, None, None)
        .unwrap();

    ticket.title = "New Title".to_string();
    ticket.body = "New Body".to_string();
    ticket.status = TicketStatus::Review;
    ticket.manager = "agent-x".to_string();
    ticket.review = true;
    ticket.branch = Some("feat/login".to_string());
    ticket.worktree = Some("/work/feat-login".to_string());
    ticket.after = Some("T-0".to_string());
    ticket.base = Some("develop".to_string());
    ticket.story = Some("STORY-10".to_string());

    db.update_ticket(&ticket).unwrap();

    let fetched = db.get_ticket(&ticket.id).unwrap();
    assert_eq!(fetched, ticket);

    let mut phantom = ticket.clone();
    phantom.id = "T-888".to_string();
    let err = db.update_ticket(&phantom);
    assert!(matches!(err, Err(RigError::TicketNotFound(id)) if id == "T-888"));
}

#[test]
fn test_list_tickets_with_filters() {
    let db = RigDatabase::open_in_memory().unwrap();

    let t1 = db
        .create_ticket("Task 1", "Body", "alice", false, None, None, Some("S-1"))
        .unwrap();
    let mut t2 = db
        .create_ticket("Task 2", "Body", "bob", false, None, None, Some("S-1"))
        .unwrap();
    let t3 = db
        .create_ticket("Task 3", "Body", "alice", false, None, None, Some("S-2"))
        .unwrap();

    db.update_ticket_status(&t2.id, TicketStatus::Done).unwrap();
    t2.status = TicketStatus::Done;

    // Default filter: include_done = false
    let active_tickets = db
        .list_tickets(&TicketFilter {
            manager: None,
            include_done: false,
            story: None,
        })
        .unwrap();
    assert_eq!(active_tickets.len(), 2);
    assert_eq!(active_tickets[0].id, t1.id);
    assert_eq!(active_tickets[1].id, t3.id);

    // Include done
    let all_tickets = db
        .list_tickets(&TicketFilter {
            manager: None,
            include_done: true,
            story: None,
        })
        .unwrap();
    assert_eq!(all_tickets.len(), 3);

    // Filter by manager alice
    let alice_tickets = db
        .list_tickets(&TicketFilter {
            manager: Some("alice"),
            include_done: true,
            story: None,
        })
        .unwrap();
    assert_eq!(alice_tickets.len(), 2);
    assert_eq!(alice_tickets[0].id, t1.id);
    assert_eq!(alice_tickets[1].id, t3.id);

    // Filter by story S-1 including done
    let story_s1 = db
        .list_tickets(&TicketFilter {
            manager: None,
            include_done: true,
            story: Some("S-1"),
        })
        .unwrap();
    assert_eq!(story_s1.len(), 2);
    assert_eq!(story_s1[0].id, t1.id);
    assert_eq!(story_s1[1].id, t2.id);

    // Filter by story S-1 active only
    let story_s1_active = db
        .list_tickets(&TicketFilter {
            manager: None,
            include_done: false,
            story: Some("S-1"),
        })
        .unwrap();
    assert_eq!(story_s1_active.len(), 1);
    assert_eq!(story_s1_active[0].id, t1.id);
}

#[test]
fn test_waiting_in_dependency_resolution() {
    let db = RigDatabase::open_in_memory().unwrap();

    let d1 = db
        .create_ticket("Dep 1", "Body", "lead", false, None, None, None)
        .unwrap();
    let d2 = db
        .create_ticket("Dep 2", "Body", "lead", false, None, None, None)
        .unwrap();

    let mut consumer = db
        .create_ticket(
            "Consumer",
            "Body",
            "lead",
            false,
            Some(&format!("{}, {}, T-999", d1.id, d2.id)),
            None,
            None,
        )
        .unwrap();

    // Initially both d1 and d2 are Open, T-999 does not exist
    let waiting = db.waiting_in(&consumer).unwrap();
    assert_eq!(waiting, vec![d1.id.clone(), d2.id.clone(), "T-999".to_string()]);

    // Mark d1 as Done
    db.update_ticket_status(&d1.id, TicketStatus::Done).unwrap();
    let waiting = db.waiting_in(&consumer).unwrap();
    assert_eq!(waiting, vec![d2.id.clone(), "T-999".to_string()]);

    // Mark d2 as Done
    db.update_ticket_status(&d2.id, TicketStatus::Done).unwrap();
    let waiting = db.waiting_in(&consumer).unwrap();
    assert_eq!(waiting, vec!["T-999".to_string()]);

    // If after is empty or None
    consumer.after = None;
    let waiting = db.waiting_in(&consumer).unwrap();
    assert!(waiting.is_empty());

    consumer.after = Some("   ".to_string());
    let waiting = db.waiting_in(&consumer).unwrap();
    assert!(waiting.is_empty());
}

#[test]
fn test_group_of_board_category_mapping() {
    let db = RigDatabase::open_in_memory().unwrap();

    let dep = db
        .create_ticket("Prerequisite", "Body", "lead", false, None, None, None)
        .unwrap();

    let mut ticket = db
        .create_ticket(
            "Test Ticket",
            "Body",
            "lead",
            false,
            Some(&dep.id),
            None,
            None,
        )
        .unwrap();

    // Open with pending dependency -> Queued
    assert_eq!(db.group_of(&ticket).unwrap(), "Queued");

    // Draft with pending dependency -> Queued
    ticket.status = TicketStatus::Draft;
    assert_eq!(db.group_of(&ticket).unwrap(), "Queued");

    // Mark dep done -> To do
    db.update_ticket_status(&dep.id, TicketStatus::Done).unwrap();
    ticket.status = TicketStatus::Open;
    assert_eq!(db.group_of(&ticket).unwrap(), "To do");

    ticket.status = TicketStatus::Draft;
    assert_eq!(db.group_of(&ticket).unwrap(), "To do");

    // Other status mappings
    ticket.status = TicketStatus::NeedsYou;
    assert_eq!(db.group_of(&ticket).unwrap(), "Needs you");

    ticket.status = TicketStatus::Blocked;
    assert_eq!(db.group_of(&ticket).unwrap(), "Needs you");

    ticket.status = TicketStatus::Working;
    assert_eq!(db.group_of(&ticket).unwrap(), "Working");

    ticket.status = TicketStatus::Reviewing;
    assert_eq!(db.group_of(&ticket).unwrap(), "Working");

    ticket.status = TicketStatus::Review;
    assert_eq!(db.group_of(&ticket).unwrap(), "Ready");

    ticket.status = TicketStatus::Done;
    assert_eq!(db.group_of(&ticket).unwrap(), "Done");
}
