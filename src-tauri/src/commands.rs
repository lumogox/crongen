use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, State};

use crate::agent_templates;
use crate::context;
use crate::db;
use crate::git_manager;
use crate::models::{
    Agent, AgentType, AgentTypeConfig, AppSettings, DecisionNode, ExecutionMode, NodeStatus,
    OrchestratorMode, OrchestratorStatus,
};
use crate::orchestrator::OrchestratorManager;
use crate::plan_generator;
use crate::pty_manager::PtyManager;
use crate::sdk_manager::SdkManager;
use crate::toon;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub pty: Arc<PtyManager>,
    pub sdk: Arc<SdkManager>,
    pub orchestrator: Arc<OrchestratorManager>,
}

// ─── Agent CRUD ────────────────────────────────────────────────

#[tauri::command]
pub async fn create_agent(
    state: State<'_, AppState>,
    name: String,
    prompt: String,
    repo_path: String,
    agent_type: String,
    type_config: serde_json::Value,
    project_mode: Option<String>,
) -> Result<Agent, String> {
    // Parse agent type
    let at = AgentType::from_str(&agent_type)?;

    // Parse type config from JSON value
    let tc: AgentTypeConfig =
        serde_json::from_value(type_config).map_err(|e| format!("Invalid type_config: {e}"))?;

    // Resolve shell from agent type
    let shell = agent_templates::default_shell_for_type(&at).to_string();

    // Validate repo path exists
    if !std::path::Path::new(&repo_path).is_dir() {
        return Err(format!("Repository path does not exist: {repo_path}"));
    }

    let now = db::now_unix();
    let agent = Agent {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        prompt,
        shell,
        repo_path,
        is_active: true,
        agent_type: at,
        type_config: tc,
        project_mode: project_mode.unwrap_or_else(|| "blank".to_string()),
        created_at: now,
        updated_at: now,
    };

    let db = state.db.clone();
    let agent_clone = agent.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_create(&conn, &agent_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Created agent: {} ({})", agent.name, agent.id);
    Ok(agent)
}

#[tauri::command]
pub async fn get_agents(state: State<'_, AppState>) -> Result<Vec<Agent>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_all(&conn).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn get_agent(state: State<'_, AppState>, id: String) -> Result<Agent, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn update_agent(
    state: State<'_, AppState>,
    id: String,
    name: String,
    prompt: String,
    repo_path: String,
    agent_type: String,
    type_config: serde_json::Value,
    is_active: bool,
    project_mode: Option<String>,
) -> Result<Agent, String> {
    let at = AgentType::from_str(&agent_type)?;
    let tc: AgentTypeConfig =
        serde_json::from_value(type_config).map_err(|e| format!("Invalid type_config: {e}"))?;
    let shell = agent_templates::default_shell_for_type(&at).to_string();

    if !std::path::Path::new(&repo_path).is_dir() {
        return Err(format!("Repository path does not exist: {repo_path}"));
    }

    let db = state.db.clone();
    let now = db::now_unix();

    // Fetch existing agent to preserve created_at + project_mode
    let db_clone = db.clone();
    let id_clone = id.clone();
    let existing = tokio::task::spawn_blocking(move || {
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &id_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let agent = Agent {
        id,
        name,
        prompt,
        shell,
        repo_path,
        is_active,
        agent_type: at,
        type_config: tc,
        project_mode: project_mode.unwrap_or(existing.project_mode),
        created_at: existing.created_at,
        updated_at: now,
    };

    let db_clone = db.clone();
    let agent_clone = agent.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_update(&conn, &agent_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Updated agent: {} ({})", agent.name, agent.id);
    Ok(agent)
}

#[tauri::command]
pub async fn delete_agent(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_delete(&conn, &id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn toggle_agent(
    state: State<'_, AppState>,
    id: String,
    is_active: bool,
) -> Result<Agent, String> {
    let db = state.db.clone();
    let db_clone = db.clone();
    let id_clone = id.clone();

    let mut agent = tokio::task::spawn_blocking(move || {
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &id_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    agent.is_active = is_active;
    agent.updated_at = db::now_unix();

    let agent_clone = agent.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_update(&conn, &agent_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Toggled agent {} to is_active={}", agent.name, is_active);
    Ok(agent)
}

// ─── Decision Tree (stubs for Phase 4+) ───────────────────────

#[tauri::command]
pub async fn get_decision_tree(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<DecisionNode>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_tree(&conn, &agent_id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

// ─── Git + Node Operations ────────────────────────────────────

/// Generate a slugified branch name from agent name + timestamp.
fn make_branch_name(agent_name: &str) -> String {
    let slug: String = agent_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let ts = db::now_unix();
    format!("agent-chron/{slug}/{ts}")
}

#[tauri::command]
pub async fn run_agent_now(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<DecisionNode, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // 1. Fetch the agent
    let agent = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &id).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 2. Check no active sessions (DB check + PTY check)
    let agent_id = agent.id.clone();
    let db3 = db2.clone();
    let has_active_db = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_has_active_session(&conn, &agent_id).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if has_active_db
        || state.pty.has_active_for_agent(&agent.id)
        || state.sdk.has_active_for_agent(&agent.id)
    {
        return Err("Agent already has a running or paused session".to_string());
    }

    // 3. Ensure the repo path is a git repo (auto-init if not)
    let repo_path = agent.repo_path.clone();
    tokio::task::spawn_blocking(move || {
        git_manager::ensure_git_repo(&repo_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 4. Create worktree
    let branch_name = make_branch_name(&agent.name);
    let repo_path = agent.repo_path.clone();
    let branch = branch_name.clone();
    let wt_info = tokio::task::spawn_blocking(move || {
        git_manager::create_worktree(&repo_path, &branch, None).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 5. Get the commit hash
    let wt_path = wt_info.path.clone();
    let commit_hash = tokio::task::spawn_blocking(move || {
        git_manager::get_current_commit(&wt_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 6. Create root decision node
    let now = db::now_unix();
    let node = DecisionNode {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: agent.id.clone(),
        parent_id: None,
        label: agent.name.clone(),
        prompt: agent.prompt.clone(),
        branch_name,
        worktree_path: Some(wt_info.path.clone()),
        commit_hash: Some(commit_hash),
        status: NodeStatus::Pending,
        exit_code: None,
        node_type: Some("task".to_string()),
        scheduled_at: None,
        created_at: now,
        updated_at: now,
    };

    let node_clone = node.clone();
    let db4 = db2.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db4.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_create(&conn, &node_clone).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 7. Build execution mode and spawn appropriate session (no context for root)
    let exec_model = get_settings().await.ok().and_then(|s| s.execution_model);
    let execution = agent_templates::build_shell_command(
        &agent.agent_type,
        &agent.prompt,
        &agent.type_config,
        None,
        node.node_type.as_deref(),
        exec_model.as_deref(),
    );

    match execution {
        ExecutionMode::Pty(shell) => {
            state
                .pty
                .spawn_session(
                    &node.id,
                    &agent.id,
                    &node.id,
                    &shell.program,
                    &shell.args,
                    &wt_info.path,
                    shell.stdin_injection.as_deref(),
                    shell.auto_responses,
                    db2,
                    app,
                )
                .map_err(|e| format!("Failed to spawn PTY session: {e}"))?;
        }
        ExecutionMode::Sdk(sdk) => {
            state
                .sdk
                .spawn_session(
                    &node.id,
                    &agent.id,
                    &node.id,
                    &sdk.program,
                    &sdk.args,
                    &wt_info.path,
                    db2,
                    app,
                )
                .map_err(|e| format!("Failed to spawn SDK session: {e}"))?;
        }
    }

    log::info!(
        "Started agent run: {} → node {} (branch {})",
        agent.name,
        node.id,
        node.branch_name
    );

    // Return with Running status — spawn_session already updated DB and emitted
    // the session_started event, so the node is Running by the time we return.
    let mut node = node;
    node.status = NodeStatus::Running;
    Ok(node)
}

#[tauri::command]
pub async fn fork_node(
    state: State<'_, AppState>,
    node_id: String,
    label: String,
    prompt: String,
) -> Result<DecisionNode, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // 1. Fetch parent node
    let nid = node_id.clone();
    let parent_node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 2. Fetch the agent
    let db3 = db2.clone();
    let aid = parent_node.agent_id.clone();
    let agent = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &aid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 3. Find nearest ancestor with a commit hash (structural nodes don't have one)
    let from_commit = if let Some(ref hash) = parent_node.commit_hash {
        hash.clone()
    } else {
        // Walk up parent chain to find nearest ancestor with commit_hash
        let db_walk = db2.clone();
        let mut current_id = parent_node.parent_id.clone();
        let mut found_commit: Option<String> = None;
        while let Some(pid) = current_id {
            let db_inner = db_walk.clone();
            let pid_clone = pid.clone();
            let ancestor = tokio::task::spawn_blocking(move || {
                let conn = db_inner.lock().map_err(|e| format!("DB lock error: {e}"))?;
                db::node_get_by_id(&conn, &pid_clone).map_err(|e| format!("{e}"))
            })
            .await
            .map_err(|e| format!("Task error: {e}"))??;
            if let Some(ref hash) = ancestor.commit_hash {
                found_commit = Some(hash.clone());
                break;
            }
            current_id = ancestor.parent_id;
        }
        found_commit.ok_or_else(|| "No ancestor has a commit hash — cannot fork".to_string())?
    };

    // 4. Create worktree from parent's commit
    let branch_name = make_branch_name(&label);
    let repo_path = agent.repo_path.clone();
    let branch = branch_name.clone();
    let commit = from_commit.clone();
    let wt_info = tokio::task::spawn_blocking(move || {
        git_manager::create_worktree(&repo_path, &branch, Some(&commit)).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 5. Get commit hash for the new worktree
    let wt_path = wt_info.path.clone();
    let commit_hash = tokio::task::spawn_blocking(move || {
        git_manager::get_current_commit(&wt_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 6. Create child decision node
    let now = db::now_unix();
    let node = DecisionNode {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: agent.id.clone(),
        parent_id: Some(node_id),
        label,
        prompt,
        branch_name,
        worktree_path: Some(wt_info.path),
        commit_hash: Some(commit_hash),
        status: NodeStatus::Pending,
        exit_code: None,
        node_type: Some("agent".to_string()),
        scheduled_at: None,
        created_at: now,
        updated_at: now,
    };

    let node_clone = node.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db2.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_create(&conn, &node_clone).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Forked node {} → {}", parent_node.id, node.id);
    Ok(node)
}

#[tauri::command]
pub async fn create_structural_node(
    state: State<'_, AppState>,
    agent_id: String,
    parent_id: Option<String>,
    label: String,
    prompt: String,
    node_type: String,
) -> Result<DecisionNode, String> {
    // Validate node_type
    if !["task", "decision", "agent", "merge", "final"].contains(&node_type.as_str()) {
        return Err(format!(
            "Invalid structural node type: {node_type}. Must be task, decision, agent, merge, or final"
        ));
    }

    let now = db::now_unix();
    let id = uuid::Uuid::new_v4().to_string();
    let branch_name = format!("structural/{}/{}", node_type, id);

    let node = DecisionNode {
        id,
        agent_id,
        parent_id,
        label,
        prompt,
        branch_name,
        worktree_path: None,
        commit_hash: None,
        status: NodeStatus::Pending,
        exit_code: None,
        node_type: Some(node_type),
        scheduled_at: None,
        created_at: now,
        updated_at: now,
    };

    let node_clone = node.clone();
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_create(&conn, &node_clone).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!(
        "Created structural node: {} (type: {})",
        node.id,
        node.node_type.as_deref().unwrap_or("unknown")
    );
    Ok(node)
}

#[tauri::command]
pub async fn merge_node_branch(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<git_manager::MergeResult, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // 1. Fetch node
    let nid = node_id.clone();
    let node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 2. Fetch agent for repo_path
    let db3 = db2.clone();
    let aid = node.agent_id.clone();
    let agent = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &aid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 3. Auto-commit any uncommitted changes in the worktree before merging.
    //    Agents don't always commit their work, so this ensures the branch
    //    tip includes all generated code.
    //    Skip if the worktree directory no longer exists (already cleaned up).
    if let Some(ref wt_path) = node.worktree_path {
        if std::path::Path::new(wt_path).exists() {
            let wt = wt_path.clone();
            let db_commit = db2.clone();
            let nid_commit = node_id.clone();
            tokio::task::spawn_blocking(move || {
                match git_manager::auto_commit_worktree(&wt) {
                    Ok(true) => {
                        // Update commit_hash in DB since we just created a new commit
                        if let Ok(hash) = git_manager::get_current_commit(&wt) {
                            if let Ok(conn) = db_commit.lock() {
                                let _ = db::node_update_commit(&conn, &nid_commit, &hash);
                            }
                        }
                    }
                    Ok(false) => {} // Clean worktree, nothing to do
                    Err(e) => log::warn!("Auto-commit failed for worktree {}: {e}", wt),
                }
            })
            .await
            .map_err(|e| format!("Task error: {e}"))?;
        }
    }

    // 4. Determine merge source: use branch name if it exists, fall back to commit hash.
    //    The branch may have been deleted by a previous merge attempt that cleaned up
    //    the worktree, but the commits are still reachable by hash.
    let repo_path = agent.repo_path.clone();
    let branch = node.branch_name.clone();
    let commit_hash_fallback = node.commit_hash.clone();
    let merge_source = {
        let repo_path = repo_path.clone();
        let branch = branch.clone();
        tokio::task::spawn_blocking(move || -> Result<String, String> {
            let repo = git2::Repository::open(&repo_path)
                .map_err(|e| format!("Failed to open repo: {e}"))?;
            if repo.find_branch(&branch, git2::BranchType::Local).is_ok() {
                Ok(branch)
            } else if let Some(hash) = commit_hash_fallback {
                log::info!("Branch {} not found, using commit hash {}", branch, hash);
                Ok(hash)
            } else {
                Err(format!(
                    "Branch {} does not exist and node has no commit hash",
                    branch
                ))
            }
        })
        .await
        .map_err(|e| format!("Task error: {e}"))??
    };

    // 5. Perform the merge
    let mut result = tokio::task::spawn_blocking(move || {
        git_manager::merge_branch(&repo_path, &merge_source, None).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 6. If conflicts detected, attempt auto-resolution with Claude
    if !result.success && !result.conflict_files.is_empty() {
        log::info!(
            "Attempting auto-resolution of {} conflict(s) for node {}",
            result.conflict_files.len(),
            node_id
        );

        let repo_for_resolve = agent.repo_path.clone();
        let conflicts = result.conflict_files.clone();
        let resolution_model = get_settings()
            .await
            .ok()
            .and_then(|s| s.execution_model)
            .unwrap_or_else(|| "haiku".to_string());

        // Run Claude to resolve conflicts
        let resolution =
            resolve_merge_conflicts(&repo_for_resolve, &conflicts, &resolution_model).await;

        match resolution {
            Ok(summary) => {
                // Finalize the merge commit
                let repo_fin = agent.repo_path.clone();
                match tokio::task::spawn_blocking(move || {
                    git_manager::finalize_merge_resolution(&repo_fin)
                })
                .await
                {
                    Ok(Ok(commit_hash)) => {
                        log::info!("Auto-resolved conflicts for node {}", node_id);
                        result = git_manager::MergeResult {
                            success: true,
                            merge_commit_hash: Some(commit_hash),
                            conflict_files: conflicts,
                            auto_resolved: true,
                            resolution_summary: Some(summary),
                        };
                    }
                    _ => {
                        // Finalize failed — abort
                        let repo_abort = agent.repo_path.clone();
                        tokio::task::spawn_blocking(move || {
                            git_manager::abort_merge(&repo_abort);
                        })
                        .await
                        .ok();
                    }
                }
            }
            Err(e) => {
                log::warn!("Auto-resolution failed for node {}: {e}", node_id);
                // Abort the merge and return the original conflicts
                let repo_abort = agent.repo_path.clone();
                tokio::task::spawn_blocking(move || {
                    git_manager::abort_merge(&repo_abort);
                })
                .await
                .ok();
            }
        }
    } else if !result.success {
        // No conflicts to resolve — just abort
        let repo_abort = agent.repo_path.clone();
        tokio::task::spawn_blocking(move || {
            git_manager::abort_merge(&repo_abort);
        })
        .await
        .ok();
    }

    // 7. If merge succeeded, update node status and clean up worktree
    if result.success {
        let db4 = db2.clone();
        let nid = node_id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db4.lock().map_err(|e| format!("DB lock error: {e}"))?;
            db::node_update_status(&conn, &nid, &NodeStatus::Merged, None)
                .map_err(|e| format!("{e}"))
        })
        .await
        .map_err(|e| format!("Task error: {e}"))??;

        // Remove the worktree (branch is already merged)
        if let Some(wt_path) = &node.worktree_path {
            let repo = agent.repo_path.clone();
            let wt = wt_path.clone();
            let _ =
                tokio::task::spawn_blocking(move || git_manager::remove_worktree(&repo, &wt, true))
                    .await;
        }

        log::info!("Merged node {} branch {}", node.id, node.branch_name);
    }

    Ok(result)
}

#[tauri::command]
pub async fn get_merge_preview(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<git_manager::MergePreview, String> {
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let node = db::node_get_by_id(&conn, &node_id).map_err(|e| format!("{e}"))?;
        let agent = db::agent_get_by_id(&conn, &node.agent_id).map_err(|e| format!("{e}"))?;

        git_manager::get_merge_preview(&agent.repo_path, &node.branch_name, None)
            .map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn delete_node_branch(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Vec<String>, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // 1. Fetch node + agent
    let nid = node_id.clone();
    let node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let db3 = db2.clone();
    let aid = node.agent_id.clone();
    let agent = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &aid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 2. Remove worktree if it exists
    if let Some(wt_path) = &node.worktree_path {
        let repo = agent.repo_path.clone();
        let wt = wt_path.clone();
        let _ = tokio::task::spawn_blocking(move || git_manager::remove_worktree(&repo, &wt, true))
            .await;
    }

    // 3. Delete node + descendants from DB
    let nid = node_id.clone();
    let deleted_ids = tokio::task::spawn_blocking(move || {
        let conn = db2.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_delete_branch(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!(
        "Deleted branch from node {}: {} nodes removed",
        node_id,
        deleted_ids.len()
    );
    Ok(deleted_ids)
}

#[tauri::command]
pub async fn create_root_node(
    state: State<'_, AppState>,
    agent_id: String,
    label: String,
    prompt: String,
) -> Result<DecisionNode, String> {
    let now = db::now_unix();
    let id = uuid::Uuid::new_v4().to_string();
    let branch_name = format!("pending/{}", id);

    let node = DecisionNode {
        id,
        agent_id,
        parent_id: None,
        label,
        prompt,
        branch_name,
        worktree_path: None,
        commit_hash: None,
        status: NodeStatus::Pending,
        exit_code: None,
        node_type: Some("task".to_string()),
        scheduled_at: None,
        created_at: now,
        updated_at: now,
    };

    let node_clone = node.clone();
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_create(&conn, &node_clone).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Created root node: {} ({})", node.label, node.id);
    Ok(node)
}

#[tauri::command]
pub async fn run_node(
    state: State<'_, AppState>,
    app: AppHandle,
    node_id: String,
) -> Result<DecisionNode, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // 1. Fetch the node
    let nid = node_id.clone();
    let mut node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if node.status != NodeStatus::Pending {
        return Err(format!("Node {} is not pending — cannot run", node.id));
    }

    // 2. Fetch the agent
    let db3 = db2.clone();
    let aid = node.agent_id.clone();
    let agent = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::agent_get_by_id(&conn, &aid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 3. Ensure the repo path is a git repo
    let repo_path = agent.repo_path.clone();
    tokio::task::spawn_blocking(move || {
        git_manager::ensure_git_repo(&repo_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 4. Determine base commit: HEAD for root, walk ancestors for child
    let from_commit = if node.parent_id.is_none() {
        // Root node — branch from HEAD
        None
    } else {
        // Walk up parent chain to find nearest ancestor with commit_hash
        let db_walk = db2.clone();
        let mut current_id = node.parent_id.clone();
        let mut found_commit: Option<String> = None;
        while let Some(pid) = current_id {
            let db_inner = db_walk.clone();
            let pid_clone = pid.clone();
            let ancestor = tokio::task::spawn_blocking(move || {
                let conn = db_inner.lock().map_err(|e| format!("DB lock error: {e}"))?;
                db::node_get_by_id(&conn, &pid_clone).map_err(|e| format!("{e}"))
            })
            .await
            .map_err(|e| format!("Task error: {e}"))??;
            if let Some(ref hash) = ancestor.commit_hash {
                found_commit = Some(hash.clone());
                break;
            }
            current_id = ancestor.parent_id;
        }
        Some(found_commit.ok_or_else(|| "No ancestor has a commit hash — cannot run".to_string())?)
    };

    // 5. Create worktree
    let branch_name = make_branch_name(&node.label);
    let repo_path = agent.repo_path.clone();
    let branch = branch_name.clone();
    let commit_ref = from_commit.clone();
    let wt_info = tokio::task::spawn_blocking(move || {
        git_manager::create_worktree(&repo_path, &branch, commit_ref.as_deref())
            .map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 6. Get commit hash
    let wt_path = wt_info.path.clone();
    let commit_hash = tokio::task::spawn_blocking(move || {
        git_manager::get_current_commit(&wt_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 7. Update the node in DB with worktree info
    node.branch_name = branch_name;
    node.worktree_path = Some(wt_info.path.clone());
    node.commit_hash = Some(commit_hash);

    let db4 = db2.clone();
    let nid = node.id.clone();
    let bn = node.branch_name.clone();
    let wtp = node.worktree_path.clone();
    let ch = node.commit_hash.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db4.lock().map_err(|e| format!("DB lock error: {e}"))?;
        conn.execute(
            "UPDATE decision_nodes SET branch_name=?1, worktree_path=?2, commit_hash=?3, updated_at=?4 WHERE id=?5",
            rusqlite::params![bn, wtp, ch, db::now_unix(), nid],
        ).map_err(|e| format!("DB error: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 8. Build TOON context (ancestor chain + siblings) for child nodes
    let toon_context = {
        let db5 = db2.clone();
        let node_for_ctx = node.clone();
        let repo_path_for_ctx = agent.repo_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db5.lock().map_err(|e| format!("DB lock error: {e}"))?;
            let ctx =
                context::build_execution_context(&conn, &node_for_ctx, Some(&repo_path_for_ctx))
                    .map_err(|e| format!("Context build error: {e}"))?;
            toon::build_context_string(&ctx)
        })
        .await
        .map_err(|e| format!("Task error: {e}"))??
    };

    // 9. Build execution and spawn session with TOON context
    let exec_model2 = get_settings().await.ok().and_then(|s| s.execution_model);
    let execution = agent_templates::build_shell_command(
        &agent.agent_type,
        &node.prompt,
        &agent.type_config,
        Some(&toon_context),
        node.node_type.as_deref(),
        exec_model2.as_deref(),
    );

    match execution {
        ExecutionMode::Pty(shell) => {
            state
                .pty
                .spawn_session(
                    &node.id,
                    &agent.id,
                    &node.id,
                    &shell.program,
                    &shell.args,
                    &wt_info.path,
                    shell.stdin_injection.as_deref(),
                    shell.auto_responses,
                    db2,
                    app,
                )
                .map_err(|e| format!("Failed to spawn PTY session: {e}"))?;
        }
        ExecutionMode::Sdk(sdk) => {
            state
                .sdk
                .spawn_session(
                    &node.id,
                    &agent.id,
                    &node.id,
                    &sdk.program,
                    &sdk.args,
                    &wt_info.path,
                    db2,
                    app,
                )
                .map_err(|e| format!("Failed to spawn SDK session: {e}"))?;
        }
    }

    log::info!("Running node: {} → {}", node.label, node.id);
    node.status = NodeStatus::Running;
    Ok(node)
}

#[tauri::command]
pub async fn update_node(
    state: State<'_, AppState>,
    node_id: String,
    label: String,
    prompt: String,
) -> Result<DecisionNode, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // Update content
    let nid = node_id.clone();
    let lbl = label.clone();
    let pmt = prompt.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_update_content(&conn, &nid, &lbl, &pmt).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // Fetch updated node
    let nid = node_id.clone();
    let updated = tokio::task::spawn_blocking(move || {
        let conn = db2.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Updated node content: {} ({})", updated.label, updated.id);
    Ok(updated)
}

#[tauri::command]
pub async fn get_root_nodes(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<DecisionNode>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_roots(&conn, &agent_id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

// ─── Utility Commands ──────────────────────────────────────────

#[tauri::command]
pub async fn check_executable(name: String) -> Result<bool, String> {
    // Check if executable exists on PATH
    let result = tokio::task::spawn_blocking(move || which::which(&name).is_ok())
        .await
        .map_err(|e| format!("Task error: {e}"))?;
    Ok(result)
}

#[tauri::command]
pub async fn check_env_var(name: String) -> Result<bool, String> {
    // Returns whether the env var is set (never exposes the value)
    Ok(std::env::var(&name).is_ok())
}

// ─── PTY Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn write_pty(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    state
        .pty
        .write(&session_id, data.as_bytes())
        .map_err(|e| format!("PTY write error: {e}"))
}

#[tauri::command]
pub async fn resize_pty(
    state: State<'_, AppState>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    state
        .pty
        .resize(&session_id, rows, cols)
        .map_err(|e| format!("PTY resize error: {e}"))
}

#[tauri::command]
pub async fn get_session_output(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<String>, String> {
    Ok(state.pty.get_buffered_output(&session_id))
}

#[tauri::command]
pub async fn pause_session(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
) -> Result<(), String> {
    // Try PTY first, then SDK
    let pause_result = state
        .pty
        .pause_session(&session_id)
        .or_else(|_| state.sdk.pause_session(&session_id));
    pause_result.map_err(|e| format!("Pause error: {e}"))?;

    // Update node status in DB
    let db = state.db.clone();
    let sid = session_id.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_update_status(&conn, &sid, &NodeStatus::Paused, None)
            .map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let _ = app.emit(
        "session_paused",
        serde_json::json!({ "node_id": session_id }),
    );
    Ok(())
}

#[tauri::command]
pub async fn resume_session(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
) -> Result<(), String> {
    // Try PTY first, then SDK
    let resume_result = state
        .pty
        .resume_session(&session_id)
        .or_else(|_| state.sdk.resume_session(&session_id));
    resume_result.map_err(|e| format!("Resume error: {e}"))?;

    // Update node status in DB
    let db = state.db.clone();
    let sid = session_id.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_update_status(&conn, &sid, &NodeStatus::Running, None)
            .map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let _ = app.emit(
        "session_resumed",
        serde_json::json!({ "node_id": session_id }),
    );
    Ok(())
}

// ─── SDK Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_sdk_session_output(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, String> {
    Ok(state
        .sdk
        .get_buffered_output(&session_id)
        .unwrap_or_default())
}

// ─── Orchestrator Commands ────────────────────────────────────

#[tauri::command]
pub async fn start_orchestrator(
    state: State<'_, AppState>,
    app: AppHandle,
    session_root_id: String,
    mode: String,
) -> Result<(), String> {
    let orch_mode = OrchestratorMode::from_str(&mode)?;

    state
        .orchestrator
        .start_session(
            session_root_id,
            orch_mode,
            state.db.clone(),
            state.pty.clone(),
            state.sdk.clone(),
            app,
        )
        .await
}

#[tauri::command]
pub async fn get_orchestrator_status(
    state: State<'_, AppState>,
    session_root_id: String,
) -> Result<Option<OrchestratorStatus>, String> {
    Ok(state.orchestrator.get_status(&session_root_id).await)
}

#[tauri::command]
pub async fn submit_orchestrator_decision(
    state: State<'_, AppState>,
    session_root_id: String,
    selected_node_id: String,
) -> Result<(), String> {
    state
        .orchestrator
        .submit_decision(&session_root_id, selected_node_id)
        .await
}

#[tauri::command]
pub async fn cancel_orchestrator(
    state: State<'_, AppState>,
    session_root_id: String,
) -> Result<(), String> {
    state.orchestrator.cancel_session(&session_root_id).await
}

// ─── Settings Commands ─────────────────────────────────────────

fn settings_path() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot resolve home directory".to_string())?;
    Ok(home.join(".agentcron").join("settings.json"))
}

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let data =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read settings: {e}"))?;
    let settings: AppSettings =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse settings: {e}"))?;
    Ok(settings)
}

#[tauri::command]
pub async fn update_settings(settings: AppSettings) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {e}"))?;
    }
    let data = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("Failed to write settings: {e}"))?;
    Ok(())
}

// ─── Mark Node Merged ─────────────────────────────────────────

#[tauri::command]
pub async fn mark_node_merged(state: State<'_, AppState>, node_id: String) -> Result<(), String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_update_status(&conn, &node_id, &NodeStatus::Merged, None)
            .map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;
    Ok(())
}

// ─── Merge Conflict Resolution ────────────────────────────────

/// Run Claude to resolve merge conflicts in-place.
/// Returns a summary of what was resolved.
async fn resolve_merge_conflicts(
    repo_path: &str,
    conflict_files: &[String],
    model: &str,
) -> Result<String, String> {
    let file_list = conflict_files.join(", ");
    let prompt = format!(
        "You are resolving git merge conflicts in this repository.\n\n\
         Conflicting files: {file_list}\n\n\
         Instructions:\n\
         1. Read each conflicting file\n\
         2. Look for conflict markers: <<<<<<< HEAD, =======, >>>>>>> branch\n\
         3. Resolve each conflict by choosing the best combination of both sides\n\
         4. Save each resolved file (remove ALL conflict markers)\n\
         5. After resolving all files, output a brief summary of what you chose for each file\n\n\
         Do NOT run git add or git commit — just fix the files."
    );

    let output = tokio::process::Command::new("claude")
        .args([
            "-p",
            &prompt,
            "--model",
            model,
            "--output-format",
            "text",
            "--dangerously-skip-permissions",
        ])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| format!("Failed to spawn claude for conflict resolution: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude conflict resolution failed: {stderr}"));
    }

    let summary = String::from_utf8_lossy(&output.stdout).to_string();

    // Truncate summary for storage (keep first 500 chars)
    let summary_short = if summary.len() > 500 {
        format!("{}...", &summary[..500])
    } else {
        summary
    };

    Ok(summary_short)
}

// ─── Node Reset Command ──────────────────────────────────────

#[tauri::command]
pub async fn reset_node_status(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<DecisionNode, String> {
    let db = state.db.clone();
    let db2 = db.clone();
    let nid = node_id.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_update_status(&conn, &nid, &NodeStatus::Pending, None)
            .map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let nid = node_id.clone();
    let updated = tokio::task::spawn_blocking(move || {
        let conn = db2.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Reset node {} status to pending", node_id);
    Ok(updated)
}

// ─── Plan Generation Commands ─────────────────────────────────

#[tauri::command]
pub async fn generate_plan(
    state: State<'_, AppState>,
    agent_id: String,
    prompt: String,
    complexity: Option<String>,
) -> Result<Vec<DecisionNode>, String> {
    // Look up agent to get project_mode, but treat as "existing" if any
    // session has already completed (the project is no longer blank).
    let db_mode = state.db.clone();
    let aid = agent_id.clone();
    let project_mode = tokio::task::spawn_blocking(move || {
        let conn = db_mode.lock().map_err(|e| format!("DB lock: {e}"))?;
        let agent = db::agent_get_by_id(&conn, &aid).map_err(|e| format!("{e}"))?;
        if agent.project_mode == "blank" {
            let roots = db::node_get_roots(&conn, &aid).map_err(|e| format!("{e}"))?;
            let has_completed = roots.iter().any(|r| {
                r.status == crate::models::NodeStatus::Completed
                    || r.status == crate::models::NodeStatus::Merged
            });
            if has_completed {
                return Ok::<String, String>("existing".to_string());
            }
        }
        Ok(agent.project_mode)
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // Load planning model from settings
    let planning_model = get_settings().await.ok().and_then(|s| s.planning_model);

    // Generate the plan via Claude CLI
    let complexity_str = complexity.as_deref().unwrap_or("branching");
    let plan = plan_generator::generate_plan(
        &prompt,
        &project_mode,
        planning_model.as_deref(),
        complexity_str,
    )
    .await?;

    // Convert to nodes
    let nodes = plan_generator::plan_to_nodes(&plan, &agent_id);

    // Batch insert into DB
    let db = state.db.clone();
    let nodes_clone = nodes.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        for node in &nodes_clone {
            db::node_create(&conn, node).map_err(|e| format!("DB error: {e}"))?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!(
        "Generated plan with {} nodes for agent {}",
        nodes.len(),
        agent_id
    );
    Ok(nodes)
}

// ─── Git Branch Info ──────────────────────────────────────────

#[tauri::command]
pub async fn get_repo_branch(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<String, String> {
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let agent = db::agent_get_by_id(&conn, &agent_id).map_err(|e| format!("{e}"))?;
        git_manager::get_default_branch(&agent.repo_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn create_feature_branch(
    state: State<'_, AppState>,
    node_id: String,
    branch_name: String,
) -> Result<String, String> {
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let node = db::node_get_by_id(&conn, &node_id).map_err(|e| format!("{e}"))?;
        let agent = db::agent_get_by_id(&conn, &node.agent_id).map_err(|e| format!("{e}"))?;

        let commit = node
            .commit_hash
            .as_deref()
            .ok_or_else(|| "Node has no commit hash — run the session first".to_string())?;

        git_manager::create_branch_at(&agent.repo_path, &branch_name, commit)
            .map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

// ─── Debug: TOON Context ──────────────────────────────────────

#[tauri::command]
pub async fn get_node_context(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<String, String> {
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let node = db::node_get_by_id(&conn, &node_id).map_err(|e| format!("{e}"))?;
        let agent = db::agent_get_by_id(&conn, &node.agent_id).map_err(|e| format!("{e}"))?;
        let ctx = context::build_execution_context(&conn, &node, Some(&agent.repo_path))
            .map_err(|e| format!("Context build error: {e}"))?;
        toon::build_context_string(&ctx)
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}
