use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, State};
use tokio::process::Command;

use crate::agent_templates;
use crate::context;
use crate::db;
use crate::git_manager;
use crate::models::{
    AgentProviderReadiness, AgentProviderStatus, AgentType, AgentTypeConfig, AppSettings,
    DecisionNode, ExecutionMode, NodeRuntimeValidation, NodeStatus, OrchestratorMode,
    OrchestratorStatus, Project,
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

#[derive(Debug)]
struct ResolutionInvocation {
    program: String,
    args: Vec<String>,
    output_file: Option<PathBuf>,
}

fn agent_label(agent_type: &AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "Claude Code",
        AgentType::Codex => "Codex",
        AgentType::Gemini => "Gemini",
        AgentType::Custom => "Custom",
    }
}

fn readiness_message(
    role: &str,
    agent_type: &AgentType,
    status: &AgentProviderReadiness,
) -> String {
    let label = agent_label(agent_type);
    let suffix = status
        .detail
        .as_deref()
        .map(|detail| format!(" {detail}"))
        .unwrap_or_default();

    match status.status {
        AgentProviderStatus::Ready => format!("{label} is ready for {role}."),
        AgentProviderStatus::MissingCli => {
            format!("{label} CLI is not installed. Open Agent Bay to finish {role} setup.{suffix}")
        }
        AgentProviderStatus::NeedsLogin => {
            format!("{label} needs login before it can handle {role}. Open Agent Bay to continue.{suffix}")
        }
        AgentProviderStatus::ComingSoon => {
            format!(
                "{label} {role} support is coming soon. Choose Claude Code or Codex in Agent Bay."
            )
        }
        AgentProviderStatus::Error => {
            format!(
                "{label} could not be validated for {role}. Open Agent Bay for details.{suffix}"
            )
        }
    }
}

fn parse_claude_auth_logged_in(stdout: &str) -> Option<bool> {
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    parsed.get("loggedIn").and_then(|value| value.as_bool())
}

fn classify_codex_login_status(stdout: &str, stderr: &str, success: bool) -> AgentProviderStatus {
    if success {
        return AgentProviderStatus::Ready;
    }

    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    if combined.contains("not logged in")
        || combined.contains("login required")
        || combined.contains("authentication required")
    {
        AgentProviderStatus::NeedsLogin
    } else {
        AgentProviderStatus::Error
    }
}

async fn get_provider_readiness(agent_type: &AgentType) -> AgentProviderReadiness {
    match agent_type {
        AgentType::ClaudeCode => {
            let executable = which::which("claude");
            if executable.is_err() {
                return AgentProviderReadiness::new(
                    AgentType::ClaudeCode,
                    AgentProviderStatus::MissingCli,
                    Some("Install the `claude` CLI and reopen Agent Bay.".to_string()),
                    true,
                    true,
                    false,
                );
            }

            match Command::new("claude")
                .args(["auth", "status"])
                .output()
                .await
            {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    match parse_claude_auth_logged_in(&stdout) {
                        Some(true) => AgentProviderReadiness::new(
                            AgentType::ClaudeCode,
                            AgentProviderStatus::Ready,
                            Some("Claude CLI is installed and authenticated.".to_string()),
                            true,
                            true,
                            false,
                        ),
                        Some(false) => AgentProviderReadiness::new(
                            AgentType::ClaudeCode,
                            AgentProviderStatus::NeedsLogin,
                            Some("Run `claude auth login` to connect Claude Code.".to_string()),
                            true,
                            true,
                            false,
                        ),
                        None => AgentProviderReadiness::new(
                            AgentType::ClaudeCode,
                            AgentProviderStatus::Error,
                            Some(format!(
                                "Could not parse `claude auth status`. {}{}",
                                stdout.trim(),
                                if stderr.trim().is_empty() {
                                    "".to_string()
                                } else {
                                    format!(" {}", stderr.trim())
                                }
                            )),
                            true,
                            true,
                            false,
                        ),
                    }
                }
                Err(err) => AgentProviderReadiness::new(
                    AgentType::ClaudeCode,
                    AgentProviderStatus::Error,
                    Some(format!("Failed to run `claude auth status`: {err}")),
                    true,
                    true,
                    false,
                ),
            }
        }
        AgentType::Codex => {
            let executable = which::which("codex");
            if executable.is_err() {
                return AgentProviderReadiness::new(
                    AgentType::Codex,
                    AgentProviderStatus::MissingCli,
                    Some("Install the `codex` CLI and reopen Agent Bay.".to_string()),
                    true,
                    true,
                    false,
                );
            }

            match Command::new("codex")
                .args(["login", "status"])
                .output()
                .await
            {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let status =
                        classify_codex_login_status(&stdout, &stderr, output.status.success());
                    let detail = match status {
                        AgentProviderStatus::Ready => Some(stdout.trim().to_string()),
                        AgentProviderStatus::NeedsLogin => {
                            Some("Run `codex login` to connect Codex.".to_string())
                        }
                        AgentProviderStatus::Error => Some(format!(
                            "Unexpected `codex login status` response. {}{}",
                            stdout.trim(),
                            if stderr.trim().is_empty() {
                                "".to_string()
                            } else {
                                format!(" {}", stderr.trim())
                            }
                        )),
                        AgentProviderStatus::MissingCli | AgentProviderStatus::ComingSoon => None,
                    };
                    AgentProviderReadiness::new(AgentType::Codex, status, detail, true, true, false)
                }
                Err(err) => AgentProviderReadiness::new(
                    AgentType::Codex,
                    AgentProviderStatus::Error,
                    Some(format!("Failed to run `codex login status`: {err}")),
                    true,
                    true,
                    false,
                ),
            }
        }
        AgentType::Gemini => AgentProviderReadiness::new(
            AgentType::Gemini,
            AgentProviderStatus::ComingSoon,
            Some("Gemini support is staged for a future release.".to_string()),
            false,
            false,
            true,
        ),
        AgentType::Custom => AgentProviderReadiness::new(
            AgentType::Custom,
            AgentProviderStatus::Error,
            Some("Custom shells are not part of Agent Bay defaults.".to_string()),
            false,
            false,
            false,
        ),
    }
}

fn role_requires_provider_validation(agent_type: &AgentType, role: &str) -> bool {
    !matches!((agent_type, role), (AgentType::Custom, "execution"))
}

async fn ensure_provider_ready(agent_type: &AgentType, role: &str) -> Result<(), String> {
    if !role_requires_provider_validation(agent_type, role) {
        return Ok(());
    }

    let readiness = get_provider_readiness(agent_type).await;
    if readiness.ready {
        return Ok(());
    }

    Err(readiness_message(role, agent_type, &readiness))
}

fn resolve_project_execution_model(
    project: &Project,
    settings: Option<&AppSettings>,
) -> Option<String> {
    let default_model = settings.and_then(|entry| entry.execution_model.clone());
    match &project.type_config {
        AgentTypeConfig::ClaudeCode(cfg) => cfg.model.clone().or(default_model),
        AgentTypeConfig::Codex(cfg) => cfg.model.clone().or(default_model),
        AgentTypeConfig::Gemini(cfg) => cfg.model.clone().or(default_model),
        AgentTypeConfig::Custom(_) => default_model,
    }
}

fn cleanup_temp_file(path: Option<&PathBuf>) {
    if let Some(file) = path {
        let _ = std::fs::remove_file(file);
    }
}

fn build_merge_resolution_invocation(
    agent_type: &AgentType,
    repo_path: &str,
    prompt: &str,
    model: Option<&str>,
) -> Result<ResolutionInvocation, String> {
    match agent_type {
        AgentType::ClaudeCode => {
            let mut args = vec![
                "-p".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "text".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ];
            if let Some(value) = model {
                args.push("--model".to_string());
                args.push(value.to_string());
            }

            Ok(ResolutionInvocation {
                program: "claude".to_string(),
                args,
                output_file: None,
            })
        }
        AgentType::Codex => {
            let output_file = std::env::temp_dir().join(format!(
                "crongen-merge-resolution-{}.txt",
                uuid::Uuid::new_v4()
            ));
            let mut args = vec![
                "exec".to_string(),
                "--skip-git-repo-check".to_string(),
                "--sandbox".to_string(),
                "workspace-write".to_string(),
                "--output-last-message".to_string(),
                output_file.display().to_string(),
                "--cd".to_string(),
                repo_path.to_string(),
            ];
            if let Some(value) = model {
                args.push("--model".to_string());
                args.push(value.to_string());
            }
            args.push(prompt.to_string());

            Ok(ResolutionInvocation {
                program: "codex".to_string(),
                args,
                output_file: Some(output_file),
            })
        }
        AgentType::Gemini => Err("Gemini conflict auto-resolution is coming soon.".to_string()),
        AgentType::Custom => {
            Err("Custom shells do not support automatic merge conflict resolution.".to_string())
        }
    }
}

fn resolve_planning_agent(settings: &AppSettings) -> Result<AgentType, String> {
    match settings.planning_agent.clone() {
        Some(agent @ AgentType::ClaudeCode) | Some(agent @ AgentType::Codex) => Ok(agent),
        Some(AgentType::Gemini) => {
            Err("Gemini planning is coming soon. Choose Claude Code or Codex in Agent Bay.".to_string())
        }
        Some(AgentType::Custom) => {
            Err("Custom providers are not supported for planning defaults. Choose Claude Code or Codex in Agent Bay.".to_string())
        }
        None => Err("Choose a planning agent in Agent Bay before generating a plan.".to_string()),
    }
}

fn active_session_backend(
    pty: &PtyManager,
    sdk: &SdkManager,
    session_id: &str,
) -> Option<&'static str> {
    if pty.has_session(session_id) {
        Some("pty")
    } else if sdk.has_session(session_id) {
        Some("sdk")
    } else {
        None
    }
}

// ─── Project CRUD ──────────────────────────────────────────────

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    name: String,
    prompt: String,
    repo_path: String,
    agent_type: String,
    type_config: serde_json::Value,
    project_mode: Option<String>,
) -> Result<Project, String> {
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
    let project = Project {
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
    let project_clone = project.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_create(&conn, &project_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Created project: {} ({})", project.name, project.id);
    Ok(project)
}

#[tauri::command]
pub async fn get_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_all(&conn).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: String) -> Result<Project, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    id: String,
    name: String,
    prompt: String,
    repo_path: String,
    agent_type: String,
    type_config: serde_json::Value,
    is_active: bool,
    project_mode: Option<String>,
) -> Result<Project, String> {
    let at = AgentType::from_str(&agent_type)?;
    let tc: AgentTypeConfig =
        serde_json::from_value(type_config).map_err(|e| format!("Invalid type_config: {e}"))?;
    let shell = agent_templates::default_shell_for_type(&at).to_string();

    if !std::path::Path::new(&repo_path).is_dir() {
        return Err(format!("Repository path does not exist: {repo_path}"));
    }

    let db = state.db.clone();
    let now = db::now_unix();

    // Fetch existing project to preserve created_at + project_mode
    let db_clone = db.clone();
    let id_clone = id.clone();
    let existing = tokio::task::spawn_blocking(move || {
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &id_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let project = Project {
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
    let project_clone = project.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_update(&conn, &project_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!("Updated project: {} ({})", project.name, project.id);
    Ok(project)
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_delete(&conn, &id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[tauri::command]
pub async fn toggle_project(
    state: State<'_, AppState>,
    id: String,
    is_active: bool,
) -> Result<Project, String> {
    let db = state.db.clone();
    let db_clone = db.clone();
    let id_clone = id.clone();

    let mut project = tokio::task::spawn_blocking(move || {
        let conn = db_clone.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &id_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    project.is_active = is_active;
    project.updated_at = db::now_unix();

    let project_clone = project.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_update(&conn, &project_clone).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!(
        "Toggled project {} to is_active={}",
        project.name,
        is_active
    );
    Ok(project)
}

// ─── Decision Tree (stubs for Phase 4+) ───────────────────────

#[tauri::command]
pub async fn get_decision_tree(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<DecisionNode>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_tree(&conn, &project_id).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

// ─── Git + Node Operations ────────────────────────────────────

/// Generate a slugified branch name from project or node name + timestamp.
fn make_branch_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let ts = db::now_unix();
    format!("crongen/{slug}/{ts}")
}

#[tauri::command]
pub async fn run_project_now(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<DecisionNode, String> {
    let db = state.db.clone();
    let db2 = db.clone();

    // 1. Fetch the project
    let project = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &id).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    ensure_provider_ready(&project.agent_type, "execution").await?;

    // 2. Check no active sessions (DB check + PTY check)
    let project_id = project.id.clone();
    let db3 = db2.clone();
    let has_active_db = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_has_active_session(&conn, &project_id).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if has_active_db
        || state.pty.has_active_for_project(&project.id)
        || state.sdk.has_active_for_project(&project.id)
    {
        return Err("Project already has a running or paused session".to_string());
    }

    // 3. Ensure the repo path is a git repo (auto-init if not)
    let repo_path = project.repo_path.clone();
    tokio::task::spawn_blocking(move || {
        git_manager::ensure_git_repo(&repo_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 4. Create worktree
    let branch_name = make_branch_name(&project.name);
    let repo_path = project.repo_path.clone();
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
        project_id: project.id.clone(),
        parent_id: None,
        label: project.name.clone(),
        prompt: project.prompt.clone(),
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
        &project.agent_type,
        &project.prompt,
        &project.type_config,
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
                    &project.id,
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
                    &project.id,
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
        "Started project run: {} → node {} (branch {})",
        project.name,
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

    // 2. Fetch the project
    let db3 = db2.clone();
    let pid = parent_node.project_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))
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
    let repo_path = project.repo_path.clone();
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
        project_id: project.id.clone(),
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
    project_id: String,
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
        project_id,
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

    // 2. Fetch project for repo_path
    let db3 = db2.clone();
    let pid = node.project_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))
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
    let repo_path = project.repo_path.clone();
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

    // 6. If conflicts detected, attempt auto-resolution with the project's provider
    if !result.success && !result.conflict_files.is_empty() {
        log::info!(
            "Attempting auto-resolution of {} conflict(s) for node {}",
            result.conflict_files.len(),
            node_id
        );

        let repo_for_resolve = project.repo_path.clone();
        let conflicts = result.conflict_files.clone();
        let resolution_settings = get_settings().await.ok();
        let resolution_model =
            resolve_project_execution_model(&project, resolution_settings.as_ref());

        let resolution = resolve_merge_conflicts(
            &project.agent_type,
            &repo_for_resolve,
            &conflicts,
            resolution_model.as_deref(),
        )
        .await;

        match resolution {
            Ok(summary) => {
                // Finalize the merge commit
                let repo_fin = project.repo_path.clone();
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
                        let repo_abort = project.repo_path.clone();
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
                let repo_abort = project.repo_path.clone();
                tokio::task::spawn_blocking(move || {
                    git_manager::abort_merge(&repo_abort);
                })
                .await
                .ok();
            }
        }
    } else if !result.success {
        // No conflicts to resolve — just abort
        let repo_abort = project.repo_path.clone();
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
            let repo = project.repo_path.clone();
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
        let project = db::project_get_by_id(&conn, &node.project_id).map_err(|e| format!("{e}"))?;

        git_manager::get_merge_preview(&project.repo_path, &node.branch_name, None)
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

    // 1. Fetch node + project
    let nid = node_id.clone();
    let node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let db3 = db2.clone();
    let pid = node.project_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 2. Remove worktree if it exists
    if let Some(wt_path) = &node.worktree_path {
        let repo = project.repo_path.clone();
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
    project_id: String,
    label: String,
    prompt: String,
) -> Result<DecisionNode, String> {
    let now = db::now_unix();
    let id = uuid::Uuid::new_v4().to_string();
    let branch_name = format!("pending/{}", id);

    let node = DecisionNode {
        id,
        project_id,
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

    // 2. Fetch the project
    let db3 = db2.clone();
    let pid = node.project_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let conn = db3.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    ensure_provider_ready(&project.agent_type, "execution").await?;

    // 3. Ensure the repo path is a git repo
    let repo_path = project.repo_path.clone();
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
    let repo_path = project.repo_path.clone();
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
        let repo_path_for_ctx = project.repo_path.clone();
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
        &project.agent_type,
        &node.prompt,
        &project.type_config,
        Some(&toon_context),
        node.node_type.as_deref(),
        exec_model2.as_deref(),
    );

    state.pty.clear_session_artifacts(&node.id);
    state.sdk.clear_session_artifacts(&node.id);

    match execution {
        ExecutionMode::Pty(shell) => {
            state
                .pty
                .spawn_session(
                    &node.id,
                    &project.id,
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
                    &project.id,
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
    project_id: String,
) -> Result<Vec<DecisionNode>, String> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_roots(&conn, &project_id).map_err(|e| format!("DB error: {e}"))
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

#[tauri::command]
pub async fn get_agent_provider_statuses() -> Result<Vec<AgentProviderReadiness>, String> {
    let mut statuses = Vec::with_capacity(3);
    statuses.push(get_provider_readiness(&AgentType::ClaudeCode).await);
    statuses.push(get_provider_readiness(&AgentType::Codex).await);
    statuses.push(get_provider_readiness(&AgentType::Gemini).await);
    Ok(statuses)
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

    let db = state.db.clone();
    let root_id = session_root_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let root = db::node_get_by_id(&conn, &root_id).map_err(|e| format!("{e}"))?;
        db::project_get_by_id(&conn, &root.project_id).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    ensure_provider_ready(&project.agent_type, "execution").await?;

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
    Ok(home.join(".crongen").join("settings.json"))
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

/// Run the selected provider to resolve merge conflicts in-place.
/// Returns a summary of what was resolved.
async fn resolve_merge_conflicts(
    agent_type: &AgentType,
    repo_path: &str,
    conflict_files: &[String],
    model: Option<&str>,
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

    let invocation = build_merge_resolution_invocation(agent_type, repo_path, &prompt, model)?;
    let output = tokio::process::Command::new(&invocation.program)
        .args(&invocation.args)
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to spawn {} for conflict resolution: {e}",
                agent_label(agent_type)
            )
        })?;

    if !output.status.success() {
        cleanup_temp_file(invocation.output_file.as_ref());
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(format!(
            "{} conflict resolution failed: {details}",
            agent_label(agent_type)
        ));
    }

    let summary = if let Some(output_file) = &invocation.output_file {
        let contents = std::fs::read_to_string(output_file)
            .map_err(|e| format!("Failed to read conflict resolution output: {e}"));
        cleanup_temp_file(Some(output_file));
        contents?
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

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
pub async fn validate_node_runtime(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<NodeRuntimeValidation, String> {
    let db = state.db.clone();
    let db2 = db.clone();
    let nid = node_id.clone();

    let node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let session_backend = active_session_backend(state.pty.as_ref(), state.sdk.as_ref(), &node.id)
        .map(str::to_string);
    let session_active = session_backend.is_some();
    let mut reconciled = false;
    let message = if session_active {
        if !matches!(node.status, NodeStatus::Running | NodeStatus::Paused) {
            let db_update = state.db.clone();
            let nid = node.id.clone();
            tokio::task::spawn_blocking(move || {
                let conn = db_update
                    .lock()
                    .map_err(|e| format!("DB lock error: {e}"))?;
                db::node_update_status(&conn, &nid, &NodeStatus::Running, None)
                    .map_err(|e| format!("DB error: {e}"))
            })
            .await
            .map_err(|e| format!("Task error: {e}"))??;
            reconciled = true;
            format!(
                "Found an active {} session and marked the node as running again.",
                session_backend.as_deref().unwrap_or("agent")
            )
        } else if node.status == NodeStatus::Paused {
            format!(
                "The {} session is still paused. Continue it when you're ready.",
                session_backend.as_deref().unwrap_or("agent")
            )
        } else {
            format!(
                "The {} session is still active and streaming output.",
                session_backend.as_deref().unwrap_or("agent")
            )
        }
    } else if matches!(node.status, NodeStatus::Running | NodeStatus::Paused) {
        let db_update = state.db.clone();
        let nid = node.id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db_update
                .lock()
                .map_err(|e| format!("DB lock error: {e}"))?;
            db::node_update_status(&conn, &nid, &NodeStatus::Failed, Some(1))
                .map_err(|e| format!("DB error: {e}"))
        })
        .await
        .map_err(|e| format!("Task error: {e}"))??;
        state.pty.publish_completion(&node.id, Some(1));
        reconciled = true;
        "No active agent session was found. The node was marked failed so you can retry it or reset it to pending.".to_string()
    } else {
        match node.status {
            NodeStatus::Pending => "This node is idle and ready to run.".to_string(),
            NodeStatus::Failed => {
                "This node has already failed. Retry it or reset it to pending.".to_string()
            }
            NodeStatus::Completed => {
                "This node already completed. Retry it if you want a fresh pass.".to_string()
            }
            NodeStatus::Merged => {
                "This node was already merged. No runtime recovery is needed.".to_string()
            }
            NodeStatus::Running | NodeStatus::Paused => unreachable!(),
        }
    };

    let nid = node_id.clone();
    let updated = tokio::task::spawn_blocking(move || {
        let conn = db2.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    Ok(NodeRuntimeValidation {
        node: updated,
        session_active,
        session_backend,
        reconciled,
        message,
    })
}

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
    project_id: String,
    prompt: String,
    complexity: Option<String>,
) -> Result<Vec<DecisionNode>, String> {
    let settings = get_settings().await?;
    let planning_agent = resolve_planning_agent(&settings)?;
    ensure_provider_ready(&planning_agent, "planning").await?;

    // Look up project to get project_mode, but treat as "existing" if any
    // session has already completed (the project is no longer blank).
    let db_mode = state.db.clone();
    let pid = project_id.clone();
    let (project_mode, repo_path) = tokio::task::spawn_blocking(move || {
        let conn = db_mode.lock().map_err(|e| format!("DB lock: {e}"))?;
        let project = db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))?;
        if project.project_mode == "blank" {
            let roots = db::node_get_roots(&conn, &pid).map_err(|e| format!("{e}"))?;
            let has_completed = roots.iter().any(|r| {
                r.status == crate::models::NodeStatus::Completed
                    || r.status == crate::models::NodeStatus::Merged
            });
            if has_completed {
                return Ok::<(String, String), String>(("existing".to_string(), project.repo_path));
            }
        }
        Ok((project.project_mode, project.repo_path))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // Generate the plan via the selected planning provider
    let complexity_str = complexity.as_deref().unwrap_or("branching");
    let plan = plan_generator::generate_plan(
        &planning_agent,
        &prompt,
        &project_mode,
        settings.planning_model.as_deref(),
        complexity_str,
        &repo_path,
    )
    .await?;

    // Convert to nodes
    let nodes = plan_generator::plan_to_nodes(&plan, &project_id);

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
        "Generated plan with {} nodes for project {}",
        nodes.len(),
        project_id
    );
    Ok(nodes)
}

// ─── Git Branch Info ──────────────────────────────────────────

#[tauri::command]
pub async fn get_repo_branch(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<String, String> {
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let project = db::project_get_by_id(&conn, &project_id).map_err(|e| format!("{e}"))?;
        git_manager::get_default_branch(&project.repo_path).map_err(|e| format!("{e}"))
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
        let project = db::project_get_by_id(&conn, &node.project_id).map_err(|e| format!("{e}"))?;

        let commit = node
            .commit_hash
            .as_deref()
            .ok_or_else(|| "Node has no commit hash — run the session first".to_string())?;

        git_manager::create_branch_at(&project.repo_path, &branch_name, commit)
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
        let project = db::project_get_by_id(&conn, &node.project_id).map_err(|e| format!("{e}"))?;
        let ctx = context::build_execution_context(&conn, &node, Some(&project.repo_path))
            .map_err(|e| format!("Context build error: {e}"))?;
        toon::build_context_string(&ctx)
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        build_merge_resolution_invocation, classify_codex_login_status,
        parse_claude_auth_logged_in, role_requires_provider_validation,
    };
    use crate::models::{AgentProviderStatus, AgentType};

    #[test]
    fn parses_claude_auth_status_even_when_logged_out() {
        let stdout = r#"{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty"}"#;
        assert_eq!(parse_claude_auth_logged_in(stdout), Some(false));
    }

    #[test]
    fn classifies_codex_login_success_as_ready() {
        let status = classify_codex_login_status("Logged in using ChatGPT", "", true);
        assert_eq!(status, AgentProviderStatus::Ready);
    }

    #[test]
    fn custom_execution_skips_provider_validation() {
        assert!(!role_requires_provider_validation(
            &AgentType::Custom,
            "execution"
        ));
        assert!(role_requires_provider_validation(
            &AgentType::Custom,
            "planning"
        ));
        assert!(role_requires_provider_validation(
            &AgentType::Codex,
            "execution"
        ));
    }

    #[test]
    fn codex_merge_resolution_uses_output_file() {
        let invocation = build_merge_resolution_invocation(
            &AgentType::Codex,
            "/tmp",
            "Resolve the merge",
            Some("gpt-5"),
        )
        .expect("codex invocation");

        assert_eq!(invocation.program, "codex");
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "--output-last-message"));
        assert!(invocation.output_file.is_some());
    }
}
