use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::{AgentType, AgentTypeConfig, DecisionNode, NodeStatus, Project};

// ─── Initialization ────────────────────────────────────────────

pub fn db_init(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    let has_legacy_agents_table = has_table(conn, "agents")?;
    let has_legacy_agent_fk = table_has_column(conn, "decision_nodes", "agent_id")?;

    if has_legacy_agents_table || has_legacy_agent_fk {
        log::warn!("Legacy agent schema detected; resetting local database to projects schema");
        conn.execute_batch(
            "DROP TABLE IF EXISTS orchestrator_sessions;
             DROP TABLE IF EXISTS decision_nodes;
             DROP TABLE IF EXISTS agents;
             DROP TABLE IF EXISTS projects;",
        )?;
        conn.pragma_update(None, "user_version", 0)?;
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            prompt        TEXT NOT NULL,
            shell         TEXT NOT NULL,
            repo_path     TEXT NOT NULL,
            is_active     INTEGER NOT NULL DEFAULT 1,
            agent_type    TEXT NOT NULL DEFAULT 'custom',
            type_config   TEXT NOT NULL DEFAULT '{}',
            project_mode  TEXT NOT NULL DEFAULT 'blank',
            created_at    INTEGER NOT NULL,
            updated_at    INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_projects_active ON projects(is_active);

        CREATE TABLE IF NOT EXISTS decision_nodes (
            id              TEXT PRIMARY KEY,
            project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
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

        CREATE INDEX IF NOT EXISTS idx_nodes_project ON decision_nodes(project_id);
        CREATE INDEX IF NOT EXISTS idx_nodes_parent ON decision_nodes(parent_id);
        ",
    )?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS orchestrator_sessions (
            session_root_id TEXT PRIMARY KEY REFERENCES decision_nodes(id),
            mode            TEXT NOT NULL DEFAULT 'auto',
            state           TEXT NOT NULL DEFAULT 'idle',
            current_node_id TEXT,
            started_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );
        ",
    )?;

    conn.pragma_update(None, "user_version", 1)?;

    Ok(())
}

// ─── Helpers ───────────────────────────────────────────────────

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn has_table(conn: &Connection, table: &str) -> Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
    )?;
    Ok(stmt.exists(params![table])?)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    if !has_table(conn, table)? {
        return Ok(false);
    }

    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;

    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }

    Ok(false)
}

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
    let agent_type_str: String = row.get("agent_type")?;
    let type_config_str: String = row.get("type_config")?;
    let is_active_int: i32 = row.get("is_active")?;

    let agent_type = AgentType::from_str(&agent_type_str).unwrap_or(AgentType::Custom);

    let type_config: AgentTypeConfig = serde_json::from_str(&type_config_str).unwrap_or(
        AgentTypeConfig::Custom(crate::models::CustomConfig::default()),
    );

    Ok(Project {
        id: row.get("id")?,
        name: row.get("name")?,
        prompt: row.get("prompt")?,
        shell: row.get("shell")?,
        repo_path: row.get("repo_path")?,
        is_active: is_active_int != 0,
        agent_type,
        type_config,
        project_mode: row
            .get::<_, String>("project_mode")
            .unwrap_or_else(|_| "blank".to_string()),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_node(row: &rusqlite::Row) -> rusqlite::Result<DecisionNode> {
    let status_str: String = row.get("status")?;
    let status = NodeStatus::from_str(&status_str).unwrap_or(NodeStatus::Pending);

    Ok(DecisionNode {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        parent_id: row.get("parent_id")?,
        label: row.get("label")?,
        prompt: row.get("prompt")?,
        branch_name: row.get("branch_name")?,
        worktree_path: row.get("worktree_path")?,
        commit_hash: row.get("commit_hash")?,
        status,
        exit_code: row.get("exit_code")?,
        node_type: row.get("node_type")?,
        scheduled_at: row.get("scheduled_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

// ─── Project CRUD ──────────────────────────────────────────────

pub fn project_create(conn: &Connection, project: &Project) -> Result<()> {
    let type_config_json =
        serde_json::to_string(&project.type_config).context("Failed to serialize type_config")?;

    conn.execute(
        "INSERT INTO projects (id, name, prompt, shell, repo_path,
         is_active, agent_type, type_config, project_mode, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            project.id,
            project.name,
            project.prompt,
            project.shell,
            project.repo_path,
            project.is_active as i32,
            project.agent_type.as_str(),
            type_config_json,
            project.project_mode,
            project.created_at,
            project.updated_at,
        ],
    )?;

    Ok(())
}

pub fn project_get_all(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, prompt, shell, repo_path,
                is_active, agent_type, type_config, project_mode, created_at, updated_at
         FROM projects ORDER BY created_at DESC",
    )?;

    let projects = stmt
        .query_map([], row_to_project)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(projects)
}

pub fn project_get_by_id(conn: &Connection, id: &str) -> Result<Project> {
    let mut stmt = conn.prepare(
        "SELECT id, name, prompt, shell, repo_path,
                is_active, agent_type, type_config, project_mode, created_at, updated_at
         FROM projects WHERE id = ?1",
    )?;

    let project = stmt
        .query_row(params![id], row_to_project)
        .context("Project not found")?;

    Ok(project)
}

pub fn project_update(conn: &Connection, project: &Project) -> Result<()> {
    let type_config_json =
        serde_json::to_string(&project.type_config).context("Failed to serialize type_config")?;

    let rows = conn.execute(
        "UPDATE projects SET name=?1, prompt=?2, shell=?3, repo_path=?4,
         is_active=?5, agent_type=?6, type_config=?7, project_mode=?8, updated_at=?9
         WHERE id=?10",
        params![
            project.name,
            project.prompt,
            project.shell,
            project.repo_path,
            project.is_active as i32,
            project.agent_type.as_str(),
            type_config_json,
            project.project_mode,
            project.updated_at,
            project.id,
        ],
    )?;

    if rows == 0 {
        anyhow::bail!("Project not found: {}", project.id);
    }

    Ok(())
}

pub fn project_delete(conn: &Connection, id: &str) -> Result<()> {
    // Delete in FK-dependency order: orchestrator_sessions → decision_nodes → projects
    conn.execute(
        "DELETE FROM orchestrator_sessions WHERE session_root_id IN \
         (SELECT id FROM decision_nodes WHERE project_id = ?1)",
        params![id],
    )?;
    conn.execute(
        "DELETE FROM decision_nodes WHERE project_id = ?1",
        params![id],
    )?;
    let rows = conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    if rows == 0 {
        anyhow::bail!("Project not found: {id}");
    }
    Ok(())
}

// ─── Decision Node CRUD ────────────────────────────────────────

pub fn node_create(conn: &Connection, node: &DecisionNode) -> Result<()> {
    conn.execute(
        "INSERT INTO decision_nodes (id, project_id, parent_id, label, prompt,
         branch_name, worktree_path, commit_hash, status, exit_code,
         node_type, scheduled_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            node.id,
            node.project_id,
            node.parent_id,
            node.label,
            node.prompt,
            node.branch_name,
            node.worktree_path,
            node.commit_hash,
            node.status.as_str(),
            node.exit_code,
            node.node_type,
            node.scheduled_at,
            node.created_at,
            node.updated_at,
        ],
    )?;

    Ok(())
}

pub fn node_get_tree(conn: &Connection, project_id: &str) -> Result<Vec<DecisionNode>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, parent_id, label, prompt, branch_name,
                worktree_path, commit_hash, status, exit_code,
                node_type, scheduled_at, created_at, updated_at
         FROM decision_nodes WHERE project_id = ?1
         ORDER BY created_at ASC",
    )?;

    let nodes = stmt
        .query_map(params![project_id], row_to_node)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(nodes)
}

pub fn node_get_by_id(conn: &Connection, id: &str) -> Result<DecisionNode> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, parent_id, label, prompt, branch_name,
                worktree_path, commit_hash, status, exit_code,
                node_type, scheduled_at, created_at, updated_at
         FROM decision_nodes WHERE id = ?1",
    )?;

    let node = stmt
        .query_row(params![id], row_to_node)
        .context("Decision node not found")?;

    Ok(node)
}

pub fn node_update_status(
    conn: &Connection,
    id: &str,
    status: &NodeStatus,
    exit_code: Option<i32>,
) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE decision_nodes SET status=?1, exit_code=?2, updated_at=?3 WHERE id=?4",
        params![status.as_str(), exit_code, now, id],
    )?;
    Ok(())
}

pub fn node_update_commit(conn: &Connection, id: &str, commit_hash: &str) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE decision_nodes SET commit_hash=?1, updated_at=?2 WHERE id=?3",
        params![commit_hash, now, id],
    )?;
    Ok(())
}

pub fn node_has_active_session(conn: &Connection, project_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM decision_nodes
         WHERE project_id = ?1 AND status IN ('running', 'paused')",
        params![project_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn node_delete_branch(conn: &Connection, id: &str) -> Result<Vec<String>> {
    // Collect all node IDs in this branch (node + all descendants) via recursive CTE
    let mut stmt = conn.prepare(
        "WITH RECURSIVE branch(nid) AS (
            SELECT id FROM decision_nodes WHERE id = ?1
            UNION ALL
            SELECT dn.id FROM decision_nodes dn
            JOIN branch b ON dn.parent_id = b.nid
        )
        SELECT nid FROM branch",
    )?;

    let node_ids: Vec<String> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    // Delete all nodes in the branch
    for nid in &node_ids {
        conn.execute("DELETE FROM decision_nodes WHERE id = ?1", params![nid])?;
    }

    Ok(node_ids)
}

pub fn node_get_roots(conn: &Connection, project_id: &str) -> Result<Vec<DecisionNode>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, parent_id, label, prompt, branch_name,
                worktree_path, commit_hash, status, exit_code,
                node_type, scheduled_at, created_at, updated_at
         FROM decision_nodes WHERE project_id = ?1 AND parent_id IS NULL
         ORDER BY created_at DESC",
    )?;

    let nodes = stmt
        .query_map(params![project_id], row_to_node)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(nodes)
}

pub fn node_get_children(conn: &Connection, parent_id: &str) -> Result<Vec<DecisionNode>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, parent_id, label, prompt, branch_name,
                worktree_path, commit_hash, status, exit_code,
                node_type, scheduled_at, created_at, updated_at
         FROM decision_nodes WHERE parent_id = ?1
         ORDER BY created_at ASC",
    )?;

    let nodes = stmt
        .query_map(params![parent_id], row_to_node)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(nodes)
}

pub fn node_get_subtree(conn: &Connection, root_id: &str) -> Result<Vec<DecisionNode>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(nid) AS (
            SELECT id FROM decision_nodes WHERE id = ?1
            UNION ALL
            SELECT dn.id FROM decision_nodes dn
            JOIN subtree s ON dn.parent_id = s.nid
        )
        SELECT dn.id, dn.project_id, dn.parent_id, dn.label, dn.prompt,
               dn.branch_name, dn.worktree_path, dn.commit_hash, dn.status,
               dn.exit_code, dn.node_type, dn.scheduled_at, dn.created_at, dn.updated_at
        FROM decision_nodes dn
        JOIN subtree s ON dn.id = s.nid
        ORDER BY dn.created_at ASC",
    )?;

    let nodes = stmt
        .query_map(params![root_id], row_to_node)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(nodes)
}

pub fn node_update_content(conn: &Connection, id: &str, label: &str, prompt: &str) -> Result<()> {
    let now = now_unix();
    let rows = conn.execute(
        "UPDATE decision_nodes SET label=?1, prompt=?2, updated_at=?3 WHERE id=?4",
        params![label, prompt, now, id],
    )?;
    if rows == 0 {
        anyhow::bail!("Decision node not found: {id}");
    }
    Ok(())
}

// ─── Orchestrator Session CRUD ──────────────────────────────────

pub fn orchestrator_upsert(
    conn: &Connection,
    root_id: &str,
    mode: &str,
    state: &str,
    current_node_id: Option<&str>,
) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "INSERT INTO orchestrator_sessions (session_root_id, mode, state, current_node_id, started_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(session_root_id) DO UPDATE SET
           state=excluded.state, current_node_id=excluded.current_node_id, updated_at=excluded.updated_at",
        params![root_id, mode, state, current_node_id, now],
    )?;
    Ok(())
}

pub fn orchestrator_update_state(
    conn: &Connection,
    root_id: &str,
    state: &str,
    current_node_id: Option<&str>,
) -> Result<()> {
    let now = now_unix();
    conn.execute(
        "UPDATE orchestrator_sessions SET state=?1, current_node_id=?2, updated_at=?3 WHERE session_root_id=?4",
        params![state, current_node_id, now, root_id],
    )?;
    Ok(())
}
