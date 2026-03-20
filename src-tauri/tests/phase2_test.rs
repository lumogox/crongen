use rusqlite::Connection;

#[test]
fn test_db_init_and_agent_crud() {
    let conn = Connection::open_in_memory().unwrap();

    // Init schema
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS agents (
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
        CREATE INDEX IF NOT EXISTS idx_agents_active ON agents(is_active);
        CREATE TABLE IF NOT EXISTS decision_nodes (
            id              TEXT PRIMARY KEY,
            agent_id        TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
            parent_id       TEXT REFERENCES decision_nodes(id),
            label           TEXT NOT NULL,
            prompt          TEXT NOT NULL,
            branch_name     TEXT NOT NULL,
            worktree_path   TEXT,
            commit_hash     TEXT,
            status          TEXT NOT NULL DEFAULT 'pending',
            exit_code       INTEGER,
            node_type       TEXT,
            scheduled_at    TEXT,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_nodes_agent ON decision_nodes(agent_id);
        CREATE INDEX IF NOT EXISTS idx_nodes_parent ON decision_nodes(parent_id);
        ",
    ).unwrap();

    // Create agent
    let config = serde_json::json!({
        "type": "claude_code",
        "model": null,
        "max_turns": null,
        "max_budget_usd": null,
        "allowed_tools": null,
        "disallowed_tools": null,
        "append_system_prompt": null,
        "output_format": null,
        "dangerously_skip_permissions": true
    });
    let config_str = serde_json::to_string(&config).unwrap();

    conn.execute(
        "INSERT INTO agents (id, name, prompt, shell, repo_path,
         is_active, agent_type, type_config, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            "test-id-001",
            "Test Claude Agent",
            "Fix the tests",
            "claude",
            "/tmp",
            1,
            "claude_code",
            config_str,
            1000000,
            1000000,
        ],
    ).unwrap();

    // Fetch agents
    let mut stmt = conn.prepare(
        "SELECT id, name, prompt, shell, repo_path,
                is_active, agent_type, type_config, created_at, updated_at
         FROM agents"
    ).unwrap();

    let agents: Vec<(String, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>("id")?,
                row.get::<_, String>("name")?,
                row.get::<_, String>("agent_type")?,
                row.get::<_, String>("type_config")?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].0, "test-id-001");
    assert_eq!(agents[0].1, "Test Claude Agent");
    assert_eq!(agents[0].2, "claude_code");

    // Parse config back
    let parsed: serde_json::Value = serde_json::from_str(&agents[0].3).unwrap();
    assert_eq!(parsed["type"], "claude_code");
    assert_eq!(parsed["dangerously_skip_permissions"], true);

    // Update agent
    conn.execute(
        "UPDATE agents SET name = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params!["Updated Agent", 2000000, "test-id-001"],
    ).unwrap();

    let name: String = conn.query_row(
        "SELECT name FROM agents WHERE id = ?1",
        rusqlite::params!["test-id-001"],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(name, "Updated Agent");

    // Create decision node (session root with scheduled_at)
    conn.execute(
        "INSERT INTO decision_nodes (id, agent_id, parent_id, label, prompt, branch_name,
         status, node_type, scheduled_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            "node-001",
            "test-id-001",
            Option::<String>::None,
            "Root Run",
            "Fix the tests",
            "crongen/test-001/root",
            "running",
            "task",
            "2026-03-15T09:00",
            1000000,
            1000000,
        ],
    ).unwrap();

    // Verify node and scheduled_at
    let (node_status, sched): (String, Option<String>) = conn.query_row(
        "SELECT status, scheduled_at FROM decision_nodes WHERE id = ?1",
        rusqlite::params!["node-001"],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!(node_status, "running");
    assert_eq!(sched, Some("2026-03-15T09:00".to_string()));

    // Delete agent (CASCADE should delete nodes)
    conn.execute("DELETE FROM agents WHERE id = ?1", rusqlite::params!["test-id-001"]).unwrap();

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM decision_nodes WHERE agent_id = ?1",
        rusqlite::params!["test-id-001"],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 0, "CASCADE delete should have removed decision nodes");

    println!("All Phase 2 DB tests passed!");
}
