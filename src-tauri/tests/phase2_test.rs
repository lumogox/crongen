#[path = "../src/db.rs"]
mod db;
#[path = "../src/models.rs"]
mod models;

use rusqlite::Connection;

fn table_exists(conn: &Connection, table: &str) -> bool {
    let mut stmt = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1")
        .unwrap();
    stmt.exists([table]).unwrap()
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma).unwrap();
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    columns.iter().any(|name| name == column)
}

#[test]
fn test_db_init_creates_project_schema_and_crud() {
    let conn = Connection::open_in_memory().unwrap();

    db::db_init(&conn).unwrap();

    assert!(table_exists(&conn, "projects"));
    assert!(table_exists(&conn, "decision_nodes"));
    assert!(table_exists(&conn, "orchestrator_sessions"));
    assert!(column_exists(&conn, "projects", "project_mode"));
    assert!(column_exists(&conn, "decision_nodes", "project_id"));
    assert!(!column_exists(&conn, "decision_nodes", "agent_id"));

    let user_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(user_version, 1);

    let config = serde_json::json!({
        "type": "claude_code",
        "model": null,
        "max_turns": null,
        "max_budget_usd": null,
        "allowed_tools": null,
        "disallowed_tools": null,
        "append_system_prompt": null,
        "dangerously_skip_permissions": true
    });
    let config_str = serde_json::to_string(&config).unwrap();

    conn.execute(
        "INSERT INTO projects (id, name, prompt, shell, repo_path,
         is_active, agent_type, type_config, project_mode, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            "project-001",
            "Test Claude Project",
            "Fix the tests",
            "claude",
            "/tmp",
            1,
            "claude_code",
            config_str,
            "existing",
            1_000_000,
            1_000_000,
        ],
    )
    .unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT id, name, agent_type, type_config, project_mode
             FROM projects",
        )
        .unwrap();

    let projects: Vec<(String, String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("id")?,
                row.get::<_, String>("name")?,
                row.get::<_, String>("agent_type")?,
                row.get::<_, String>("type_config")?,
                row.get::<_, String>("project_mode")?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].0, "project-001");
    assert_eq!(projects[0].1, "Test Claude Project");
    assert_eq!(projects[0].2, "claude_code");
    assert_eq!(projects[0].4, "existing");

    let parsed: serde_json::Value = serde_json::from_str(&projects[0].3).unwrap();
    assert_eq!(parsed["type"], "claude_code");
    assert_eq!(parsed["dangerously_skip_permissions"], true);

    conn.execute(
        "UPDATE projects SET name = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params!["Updated Project", 2_000_000, "project-001"],
    )
    .unwrap();

    let name: String = conn
        .query_row(
            "SELECT name FROM projects WHERE id = ?1",
            rusqlite::params!["project-001"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "Updated Project");

    conn.execute(
        "INSERT INTO decision_nodes (id, project_id, parent_id, label, prompt, branch_name,
         status, node_type, scheduled_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            "node-001",
            "project-001",
            Option::<String>::None,
            "Root Run",
            "Fix the tests",
            "crongen/test-001/root",
            "running",
            "task",
            "2026-03-15T09:00",
            1_000_000,
            1_000_000,
        ],
    )
    .unwrap();

    let (node_status, sched): (String, Option<String>) = conn
        .query_row(
            "SELECT status, scheduled_at FROM decision_nodes WHERE id = ?1",
            rusqlite::params!["node-001"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(node_status, "running");
    assert_eq!(sched, Some("2026-03-15T09:00".to_string()));

    conn.execute(
        "DELETE FROM projects WHERE id = ?1",
        rusqlite::params!["project-001"],
    )
    .unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM decision_nodes WHERE project_id = ?1",
            rusqlite::params!["project-001"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "Cascade delete should have removed decision nodes"
    );
}

#[test]
fn test_db_init_resets_legacy_agent_schema() {
    let conn = Connection::open_in_memory().unwrap();

    conn.execute_batch(
        "
        PRAGMA foreign_keys=ON;

        CREATE TABLE agents (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            prompt        TEXT NOT NULL,
            shell         TEXT NOT NULL,
            repo_path     TEXT NOT NULL,
            is_active     INTEGER NOT NULL DEFAULT 1,
            agent_type    TEXT NOT NULL DEFAULT 'custom',
            type_config   TEXT NOT NULL DEFAULT '{}',
            created_at    INTEGER NOT NULL,
            updated_at    INTEGER NOT NULL
        );

        CREATE TABLE decision_nodes (
            id            TEXT PRIMARY KEY,
            agent_id      TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
            parent_id     TEXT REFERENCES decision_nodes(id),
            label         TEXT NOT NULL,
            prompt        TEXT NOT NULL,
            branch_name   TEXT NOT NULL,
            status        TEXT NOT NULL DEFAULT 'pending',
            created_at    INTEGER NOT NULL,
            updated_at    INTEGER NOT NULL
        );

        CREATE TABLE orchestrator_sessions (
            session_root_id TEXT PRIMARY KEY REFERENCES decision_nodes(id),
            mode            TEXT NOT NULL DEFAULT 'auto',
            state           TEXT NOT NULL DEFAULT 'idle',
            current_node_id TEXT,
            started_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );
        ",
    )
    .unwrap();

    conn.execute(
        "INSERT INTO agents (id, name, prompt, shell, repo_path, is_active, agent_type, type_config, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            "legacy-agent-001",
            "Legacy Agent",
            "Old prompt",
            "claude",
            "/tmp",
            1,
            "claude_code",
            "{}",
            1_000_000,
            1_000_000,
        ],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO decision_nodes (id, agent_id, parent_id, label, prompt, branch_name, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            "legacy-node-001",
            "legacy-agent-001",
            Option::<String>::None,
            "Legacy Root",
            "Old prompt",
            "legacy/root",
            "pending",
            1_000_000,
            1_000_000,
        ],
    )
    .unwrap();

    db::db_init(&conn).unwrap();

    assert!(!table_exists(&conn, "agents"));
    assert!(table_exists(&conn, "projects"));
    assert!(column_exists(&conn, "projects", "project_mode"));
    assert!(column_exists(&conn, "decision_nodes", "project_id"));
    assert!(!column_exists(&conn, "decision_nodes", "agent_id"));

    let project_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
        .unwrap();
    let node_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM decision_nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(project_count, 0);
    assert_eq!(node_count, 0);

    let user_version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(user_version, 1);
}

#[test]
fn test_default_settings_start_without_agent_defaults() {
    let settings = models::AppSettings::default();

    assert!(!settings.agent_setup_seen);
    assert!(settings.planning_agent.is_none());
    assert!(settings.execution_agent.is_none());
    assert!(settings.planning_model.is_none());
    assert!(settings.execution_model.is_none());
}
