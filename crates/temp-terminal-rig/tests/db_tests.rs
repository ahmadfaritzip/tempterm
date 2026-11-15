use temp_terminal_rig::RigDatabase;
use tempfile::tempdir;

fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    let mut stmt = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .expect("prepare table query");
    stmt.exists([table]).expect("execute table query")
}

#[test]
fn test_open_repo_database_creates_tables_and_pragmas() {
    let temp_dir = tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    let db = RigDatabase::open_repo(repo_root).expect("open repo db");
    let conn = db.connection();

    let rig_dir = repo_root.join(".rig");
    assert!(rig_dir.is_dir(), ".rig directory should exist");
    assert!(rig_dir.join("rig.db").is_file(), "rig.db file should exist");

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("query journal_mode");
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let busy_timeout: i64 = conn
        .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
        .expect("query busy_timeout");
    assert_eq!(busy_timeout, 5000);

    for table in ["tickets", "runs", "messages", "settings"] {
        assert!(
            table_exists(conn, table),
            "table {} should exist in rig.db",
            table
        );
    }
}

#[test]
fn test_open_hub_database_creates_hub_tables() {
    let temp_dir = tempdir().expect("create temp dir");
    let hub_home = temp_dir.path();

    let db = RigDatabase::open_hub(hub_home).expect("open hub db");
    let conn = db.connection();

    let rig_dir = hub_home.join(".rig");
    assert!(rig_dir.is_dir(), ".rig directory should exist");
    assert!(rig_dir.join("hub.db").is_file(), "hub.db file should exist");

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("query journal_mode");
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let busy_timeout: i64 = conn
        .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
        .expect("query busy_timeout");
    assert_eq!(busy_timeout, 5000);

    for table in ["repos", "stories", "story_repos", "messages", "settings"] {
        assert!(
            table_exists(conn, table),
            "table {} should exist in hub.db",
            table
        );
    }
}

#[test]
fn test_open_in_memory() {
    let mut db = RigDatabase::open_in_memory().expect("open in memory db");
    let conn = db.connection_mut();

    let busy_timeout: i64 = conn
        .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
        .expect("query busy_timeout");
    assert_eq!(busy_timeout, 5000);

    for table in ["tickets", "runs", "messages", "settings"] {
        assert!(
            table_exists(conn, table),
            "table {} should exist in in-memory db",
            table
        );
    }
}
