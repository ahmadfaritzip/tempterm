use temp_terminal_rig::{resolve_hub_dir, RegisteredRepo, RigDatabase, Story};
use tempfile::tempdir;

#[test]
fn test_repo_registration_listing_and_removal() {
    let temp = tempdir().expect("create temp dir");
    let db = RigDatabase::open_hub(temp.path()).expect("open hub db");

    let initial = db.list_repos().expect("list repos initially");
    assert!(initial.is_empty());

    let frontend_path = temp.path().join("frontend-app");
    let backend_path = temp.path().join("backend-api");
    std::fs::create_dir_all(&frontend_path).expect("create frontend dir");
    std::fs::create_dir_all(&backend_path).expect("create backend dir");

    db.register_repo("frontend", &frontend_path)
        .expect("register frontend");
    db.register_repo("backend", &backend_path)
        .expect("register backend");

    let repos = db.list_repos().expect("list repos");
    assert_eq!(repos.len(), 2);
    // Repos must be ordered by name ASC
    assert_eq!(repos[0].name, "backend");
    assert_eq!(repos[1].name, "frontend");
    assert_eq!(
        repos[0],
        RegisteredRepo {
            name: "backend".to_string(),
            path: repos[0].path.clone(),
        }
    );
    assert!(!repos[0].path.is_empty());
    assert!(!repos[1].path.is_empty());

    // Register with same name replaces path
    let new_backend_path = temp.path().join("backend-v2");
    std::fs::create_dir_all(&new_backend_path).expect("create backend v2 dir");
    db.register_repo("backend", &new_backend_path)
        .expect("replace backend repo");
    let repos_updated = db.list_repos().expect("list repos after replace");
    assert_eq!(repos_updated.len(), 2);
    assert_eq!(repos_updated[0].name, "backend");
    assert!(repos_updated[0].path.contains("backend-v2"));

    // Remove by name
    let removed = db.remove_repo("frontend").expect("remove frontend repo");
    assert!(removed);

    // Remove non-existent returns false
    let removed_none = db.remove_repo("frontend").expect("remove nonexistent");
    assert!(!removed_none);

    // Remove by path
    let removed_by_path = db
        .remove_repo(&repos_updated[0].path)
        .expect("remove backend by path");
    assert!(removed_by_path);

    let remaining = db.list_repos().expect("list repos empty");
    assert!(remaining.is_empty());
}

#[test]
fn test_story_crud_and_listing() {
    let temp = tempdir().expect("create temp dir");
    let db = RigDatabase::open_hub(temp.path()).expect("open hub db");

    let initial_stories = db.list_stories().expect("list stories initial");
    assert!(initial_stories.is_empty());

    let non_existent = db.get_story("STORY-100").expect("get nonexistent story");
    assert_eq!(non_existent, None);

    // Create a new story
    let story1 = db
        .upsert_story(
            "STORY-1",
            "Authentication Service",
            "Design and build JWT auth service",
        )
        .expect("create story 1");

    assert_eq!(story1.id, "STORY-1");
    assert_eq!(story1.title, "Authentication Service");
    assert_eq!(story1.body, "Design and build JWT auth service");
    assert_eq!(story1.status, "open");
    assert!(!story1.created.is_empty());
    assert_eq!(
        story1,
        Story {
            id: "STORY-1".to_string(),
            title: "Authentication Service".to_string(),
            body: "Design and build JWT auth service".to_string(),
            status: "open".to_string(),
            created: story1.created.clone(),
        }
    );

    // Query story by id
    let queried = db.get_story("STORY-1").expect("get story 1");
    assert_eq!(queried, Some(story1.clone()));

    // Create a second story
    let story2 = db
        .upsert_story(
            "STORY-2",
            "Billing Integration",
            "Integrate Stripe payments",
        )
        .expect("create story 2");
    assert_eq!(story2.id, "STORY-2");

    let all_stories = db.list_stories().expect("list all stories");
    assert_eq!(all_stories.len(), 2);
    assert!(all_stories.iter().any(|s| s.id == "STORY-1"));
    assert!(all_stories.iter().any(|s| s.id == "STORY-2"));

    // Update story 1 with new title, keeping existing body
    let updated = db
        .upsert_story("STORY-1", "Authentication Service v2", "")
        .expect("update story 1 title only");
    assert_eq!(updated.id, "STORY-1");
    assert_eq!(updated.title, "Authentication Service v2");
    assert_eq!(updated.body, "Design and build JWT auth service");
    assert_eq!(updated.created, story1.created);

    // Update story 1 with new body, keeping existing title
    let updated2 = db
        .upsert_story("STORY-1", "", "Replaced auth body details")
        .expect("update story 1 body only");
    assert_eq!(updated2.id, "STORY-1");
    assert_eq!(updated2.title, "Authentication Service v2");
    assert_eq!(updated2.body, "Replaced auth body details");
    assert_eq!(updated2.created, story1.created);
}

#[test]
fn test_story_repo_linking() {
    let temp = tempdir().expect("create temp dir");
    let db = RigDatabase::open_hub(temp.path()).expect("open hub db");

    let repos_empty = db
        .get_story_repos("STORY-ALPHA")
        .expect("get repos for unlinked story");
    assert!(repos_empty.is_empty());

    db.link_story_repo("STORY-ALPHA", "repo-backend")
        .expect("link backend");
    db.link_story_repo("STORY-ALPHA", "repo-frontend")
        .expect("link frontend");
    // Duplicate linking should be safely ignored
    db.link_story_repo("STORY-ALPHA", "repo-backend")
        .expect("link duplicate backend");

    let repos = db
        .get_story_repos("STORY-ALPHA")
        .expect("get repos for story alpha");
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0], "repo-backend");
    assert_eq!(repos[1], "repo-frontend");

    // Other story remains unlinked
    let other_repos = db
        .get_story_repos("STORY-BETA")
        .expect("get repos for story beta");
    assert!(other_repos.is_empty());
}

#[test]
fn test_hub_messaging_inbox_and_story_filtering() {
    let temp = tempdir().expect("create temp dir");
    let db = RigDatabase::open_hub(temp.path()).expect("open hub db");

    // Send messages via send_hub_message
    let msg1 = db
        .send_hub_message(
            Some("TIC-01"),
            "coordinator",
            "developer-1",
            "Please review cross-repo contract",
            Some("STORY-CORE"),
        )
        .expect("send msg 1");
    assert!(msg1.id > 0);
    assert_eq!(msg1.ticket.as_deref(), Some("TIC-01"));
    assert_eq!(msg1.frm, "coordinator");
    assert_eq!(msg1.too, "developer-1");
    assert_eq!(msg1.body, "Please review cross-repo contract");
    assert_eq!(msg1.story.as_deref(), Some("STORY-CORE"));
    assert!(!msg1.seen);
    assert!(!msg1.at.is_empty());

    let msg2 = db
        .send_hub_message(
            None,
            "coordinator",
            "developer-1",
            "General sync meeting at 14:00",
            None,
        )
        .expect("send msg 2");
    assert!(msg2.id > msg1.id);
    assert_eq!(msg2.story, None);

    let msg3 = db
        .send_hub_message(
            Some("TIC-02"),
            "coordinator",
            "developer-2",
            "Message for dev 2",
            Some("STORY-OTHER"),
        )
        .expect("send msg 3");
    assert!(msg3.id > msg2.id);

    // Read inbox without mark_seen
    let dev1_msgs = db
        .read_hub_inbox("developer-1", false, None)
        .expect("read dev1 inbox");
    assert_eq!(dev1_msgs.len(), 2);
    assert_eq!(dev1_msgs[0].id, msg1.id);
    assert_eq!(dev1_msgs[1].id, msg2.id);
    assert!(!dev1_msgs[0].seen);
    assert!(!dev1_msgs[1].seen);

    // Read inbox with story filter
    let dev1_story_msgs = db
        .read_hub_inbox("developer-1", false, Some("STORY-CORE"))
        .expect("read dev1 story filtered inbox");
    assert_eq!(dev1_story_msgs.len(), 1);
    assert_eq!(dev1_story_msgs[0].id, msg1.id);
    assert_eq!(dev1_story_msgs[0].story.as_deref(), Some("STORY-CORE"));

    // Read and mark seen
    let dev1_marked = db
        .read_hub_inbox("developer-1", true, None)
        .expect("read and mark seen");
    assert_eq!(dev1_marked.len(), 2);
    assert!(dev1_marked[0].seen);
    assert!(dev1_marked[1].seen);

    // Re-reading dev1 inbox now returns messages with seen == true
    let dev1_after = db
        .read_hub_inbox("developer-1", false, None)
        .expect("read dev1 inbox after seen");
    assert_eq!(dev1_after.len(), 2);
    assert!(dev1_after[0].seen);
    assert!(dev1_after[1].seen);

    // Dev 2 message is untouched and unseen
    let dev2_msgs = db
        .read_hub_inbox("developer-2", false, None)
        .expect("read dev2 inbox");
    assert_eq!(dev2_msgs.len(), 1);
    assert!(!dev2_msgs[0].seen);
}

#[test]
fn test_resolve_hub_dir() {
    // Test custom RIG_HOME environment variable
    let temp = tempdir().expect("create temp dir");
    let custom_home = temp.path().join("custom_rig_home");
    std::env::set_var("RIG_HOME", custom_home.to_str().expect("valid path str"));

    let resolved = resolve_hub_dir();
    assert_eq!(resolved, custom_home.join(".rig"));

    // Test when RIG_HOME already ends with .rig
    let rig_dir = custom_home.join(".rig");
    std::env::set_var("RIG_HOME", rig_dir.to_str().expect("valid path str"));
    let resolved_dot_rig = resolve_hub_dir();
    assert_eq!(resolved_dot_rig, rig_dir);

    std::env::remove_var("RIG_HOME");
}
