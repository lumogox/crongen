use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::process::Command;

use crate::agent_templates;
use crate::context;
use crate::db;
use crate::git_manager;
use crate::models::{
    AgentProviderReadiness, AgentProviderStatus, AgentType, AgentTypeConfig, AppSettings,
    ClaudeCodeConfig, CodexConfig, CodexModelCatalog, CodexModelOption, CodexReasoningLevel,
    CustomConfig, DecisionNode, ExecutionMode, GeminiConfig, NodeRuntimeValidation, NodeStatus,
    NodeTerminalSession, OrchestratorMode, OrchestratorState, OrchestratorStatus, Project,
};
use crate::orchestrator::OrchestratorManager;
use crate::plan_generator;
use crate::pty_manager::PtyManager;
use crate::sdk_manager::SdkManager;
use crate::toon;
use crate::validation;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub pty: Arc<PtyManager>,
    pub sdk: Arc<SdkManager>,
    pub orchestrator: Arc<OrchestratorManager>,
}

#[derive(Debug, Serialize)]
pub struct FeatureBranchResult {
    pub branch_name: String,
    pub commit_hash: String,
}

#[derive(Debug)]
struct ResolutionInvocation {
    program: String,
    args: Vec<String>,
    output_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct RawCodexModelCatalog {
    fetched_at: Option<String>,
    client_version: Option<String>,
    #[serde(default)]
    models: Vec<RawCodexModelOption>,
}

#[derive(Debug, Deserialize)]
struct RawCodexModelOption {
    slug: String,
    display_name: Option<String>,
    description: Option<String>,
    default_reasoning_level: Option<String>,
    #[serde(default)]
    supported_reasoning_levels: Vec<RawCodexReasoningLevel>,
    visibility: Option<String>,
    priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct RawCodexReasoningLevel {
    effort: String,
    description: Option<String>,
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

fn codex_models_cache_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot resolve home directory".to_string())?;
    Ok(home.join(".codex").join("models_cache.json"))
}

fn parse_codex_model_catalog(raw: &str) -> Result<CodexModelCatalog, String> {
    let mut parsed: RawCodexModelCatalog = serde_json::from_str(raw)
        .map_err(|err| format!("Failed to parse Codex model cache: {err}"))?;

    parsed.models.sort_by(|left, right| {
        left.priority
            .unwrap_or(i32::MAX)
            .cmp(&right.priority.unwrap_or(i32::MAX))
            .then_with(|| {
                left.display_name
                    .as_deref()
                    .unwrap_or(left.slug.as_str())
                    .cmp(right.display_name.as_deref().unwrap_or(right.slug.as_str()))
            })
    });

    let models = parsed
        .models
        .into_iter()
        .filter(|model| model.visibility.as_deref().unwrap_or("list") == "list")
        .map(|model| CodexModelOption {
            slug: model.slug.clone(),
            display_name: model.display_name.unwrap_or(model.slug),
            description: model.description,
            default_reasoning_level: model.default_reasoning_level,
            supported_reasoning_levels: model
                .supported_reasoning_levels
                .into_iter()
                .map(|level| CodexReasoningLevel {
                    effort: level.effort,
                    description: level.description,
                })
                .collect(),
        })
        .collect();

    Ok(CodexModelCatalog {
        source: "codex_models_cache".to_string(),
        fetched_at: parsed.fetched_at,
        client_version: parsed.client_version,
        models,
    })
}

fn load_codex_model_catalog() -> Result<CodexModelCatalog, String> {
    let path = codex_models_cache_path()?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
    parse_codex_model_catalog(&raw)
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
        AgentType::Gemini => {
            let executable = which::which("gemini");
            if executable.is_err() {
                return AgentProviderReadiness::new(
                    AgentType::Gemini,
                    AgentProviderStatus::MissingCli,
                    Some("Install the `gemini` CLI and reopen Agent Bay.".to_string()),
                    true,
                    true,
                    false,
                );
            }

            match Command::new("gemini").arg("--version").output().await {
                Ok(output) if output.status.success() => AgentProviderReadiness::new(
                    AgentType::Gemini,
                    AgentProviderStatus::Ready,
                    Some(format!(
                        "Gemini CLI is installed ({}). Authentication is checked by Gemini when a run starts.",
                        String::from_utf8_lossy(&output.stdout).trim()
                    )),
                    true,
                    true,
                    false,
                ),
                Ok(output) => AgentProviderReadiness::new(
                    AgentType::Gemini,
                    AgentProviderStatus::Error,
                    Some(format!(
                        "Unexpected `gemini --version` response. {}{}",
                        String::from_utf8_lossy(&output.stdout).trim(),
                        if output.stderr.is_empty() {
                            "".to_string()
                        } else {
                            format!(" {}", String::from_utf8_lossy(&output.stderr).trim())
                        }
                    )),
                    true,
                    true,
                    false,
                ),
                Err(err) => AgentProviderReadiness::new(
                    AgentType::Gemini,
                    AgentProviderStatus::Error,
                    Some(format!("Failed to run `gemini --version`: {err}")),
                    true,
                    true,
                    false,
                ),
            }
        }
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

pub(crate) async fn ensure_provider_ready(
    agent_type: &AgentType,
    role: &str,
) -> Result<(), String> {
    if !role_requires_provider_validation(agent_type, role) {
        return Ok(());
    }

    let readiness = get_provider_readiness(agent_type).await;
    if readiness.ready {
        return Ok(());
    }

    Err(readiness_message(role, agent_type, &readiness))
}

pub(crate) fn settings_agent_config(
    agent_type: &AgentType,
    settings: Option<&AppSettings>,
) -> Option<AgentTypeConfig> {
    let settings = settings?;
    match agent_type {
        AgentType::ClaudeCode => settings
            .agent_configs
            .claude_code
            .clone()
            .map(AgentTypeConfig::ClaudeCode),
        AgentType::Codex => settings
            .agent_configs
            .codex
            .clone()
            .map(AgentTypeConfig::Codex),
        AgentType::Gemini => settings
            .agent_configs
            .gemini
            .clone()
            .map(AgentTypeConfig::Gemini),
        AgentType::Custom => None,
    }
}

pub(crate) fn resolve_effective_agent_config(
    agent_type: &AgentType,
    project_config: &AgentTypeConfig,
    settings: Option<&AppSettings>,
) -> AgentTypeConfig {
    settings_agent_config(agent_type, settings).unwrap_or_else(|| project_config.clone())
}

pub(crate) fn default_agent_config(agent_type: &AgentType) -> AgentTypeConfig {
    match agent_type {
        AgentType::ClaudeCode => AgentTypeConfig::ClaudeCode(ClaudeCodeConfig {
            dangerously_skip_permissions: true,
            ..ClaudeCodeConfig::default()
        }),
        AgentType::Codex => AgentTypeConfig::Codex(CodexConfig::default()),
        AgentType::Gemini => AgentTypeConfig::Gemini(GeminiConfig {
            yolo: true,
            ..GeminiConfig::default()
        }),
        AgentType::Custom => AgentTypeConfig::Custom(CustomConfig::default()),
    }
}

pub(crate) fn effective_node_agent(
    node: &DecisionNode,
    project: &Project,
    settings: Option<&AppSettings>,
) -> AgentType {
    node.agent_type_override
        .clone()
        .or_else(|| settings.and_then(|value| value.execution_agent.clone()))
        .unwrap_or_else(|| project.agent_type.clone())
}

pub(crate) fn resolve_node_agent_config(
    agent_type: &AgentType,
    project: &Project,
    settings: Option<&AppSettings>,
) -> AgentTypeConfig {
    if let Some(config) = settings_agent_config(agent_type, settings) {
        return config;
    }

    if &project.agent_type == agent_type {
        return project.type_config.clone();
    }

    default_agent_config(agent_type)
}

fn agent_config_model(config: &AgentTypeConfig) -> Option<String> {
    match config {
        AgentTypeConfig::ClaudeCode(cfg) => cfg.model.clone(),
        AgentTypeConfig::Codex(cfg) => cfg.model.clone(),
        AgentTypeConfig::Gemini(cfg) => cfg.model.clone(),
        AgentTypeConfig::Custom(_) => None,
    }
}

fn agent_config_extra_args(config: &AgentTypeConfig) -> &[String] {
    match config {
        AgentTypeConfig::ClaudeCode(cfg) => &cfg.extra_args,
        AgentTypeConfig::Codex(cfg) => &cfg.extra_args,
        AgentTypeConfig::Gemini(cfg) => &cfg.extra_args,
        AgentTypeConfig::Custom(_) => &[],
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
    extra_args: &[String],
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
            append_extra_args(&mut args, extra_args);

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

                if matches!(value, "gpt-5-codex-mini" | "codex-1p-mini-q-20251105-ev3") {
                    args.push("-c".to_string());
                    args.push("model_reasoning_effort=\"medium\"".to_string());
                }
            }
            append_extra_args(&mut args, extra_args);
            args.push(prompt.to_string());

            Ok(ResolutionInvocation {
                program: "codex".to_string(),
                args,
                output_file: Some(output_file),
            })
        }
        AgentType::Gemini => {
            let mut args = vec![
                "--yolo".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
            ];
            if let Some(value) = model {
                args.push("--model".to_string());
                args.push(value.to_string());
            }
            append_extra_args(&mut args, extra_args);
            args.push("--prompt".to_string());
            args.push(prompt.to_string());

            Ok(ResolutionInvocation {
                program: "gemini".to_string(),
                args,
                output_file: None,
            })
        }
        AgentType::Custom => {
            Err("Custom shells do not support automatic merge conflict resolution.".to_string())
        }
    }
}

fn append_extra_args(args: &mut Vec<String>, extra_args: &[String]) {
    args.extend(
        extra_args
            .iter()
            .map(|arg| arg.trim())
            .filter(|arg| !arg.is_empty())
            .map(ToString::to_string),
    );
}

fn resolve_planning_agent(settings: &AppSettings) -> Result<AgentType, String> {
    match settings.planning_agent.clone() {
        Some(agent @ AgentType::ClaudeCode) | Some(agent @ AgentType::Codex) => Ok(agent),
        Some(agent @ AgentType::Gemini) => Ok(agent),
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

fn manual_terminal_session_id(node_id: &str) -> String {
    format!("manual-terminal:{node_id}")
}

fn node_has_active_runtime(state: &AppState, node: &DecisionNode) -> bool {
    matches!(node.status, NodeStatus::Running | NodeStatus::Paused)
        || active_session_backend(state.pty.as_ref(), state.sdk.as_ref(), &node.id).is_some()
        || active_session_backend(
            state.pty.as_ref(),
            state.sdk.as_ref(),
            &manual_terminal_session_id(&node.id),
        )
        .is_some()
}

fn resolve_node_terminal_cwd(node: &DecisionNode, project: &Project) -> String {
    if let Some(worktree_path) = node.worktree_path.as_ref() {
        if std::path::Path::new(worktree_path).exists() {
            return worktree_path.clone();
        }
    }

    project.repo_path.clone()
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

async fn persist_node_runtime_fields(
    db_handle: Arc<Mutex<Connection>>,
    node: &DecisionNode,
) -> Result<(), String> {
    let nid = node.id.clone();
    let branch_name = node.branch_name.clone();
    let worktree_path = node.worktree_path.clone();
    let commit_hash = node.commit_hash.clone();
    let prompt = node.prompt.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db_handle
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        conn.execute(
            "UPDATE decision_nodes
             SET prompt=?1, branch_name=?2, worktree_path=?3, commit_hash=?4, updated_at=?5
             WHERE id=?6",
            rusqlite::params![
                prompt,
                branch_name,
                worktree_path,
                commit_hash,
                db::now_unix(),
                nid
            ],
        )
        .map_err(|e| format!("DB error: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
}

async fn create_post_merge_validation_node(
    state: &AppState,
    app: &AppHandle,
    merged_node: &DecisionNode,
    project: &Project,
) -> Result<DecisionNode, String> {
    let db_children = state.db.clone();
    let parent_id = merged_node.id.clone();
    if let Some(existing) = tokio::task::spawn_blocking(move || {
        let conn = db_children
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        let children = db::node_get_children(&conn, &parent_id).map_err(|e| format!("{e}"))?;
        Ok::<Option<DecisionNode>, String>(
            children
                .into_iter()
                .find(|child| child.node_type.as_deref() == Some("validation")),
        )
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??
    {
        return Ok(existing);
    }

    let repo_path = project.repo_path.clone();
    let validation_plan =
        tokio::task::spawn_blocking(move || validation::build_validation_plan(&repo_path))
            .await
            .map_err(|e| format!("Task error: {e}"))?;

    let repo_for_branch = project.repo_path.clone();
    let branch_name =
        tokio::task::spawn_blocking(move || git_manager::get_default_branch(&repo_for_branch))
            .await
            .map_err(|e| format!("Task error: {e}"))?
            .unwrap_or_else(|_| "main".to_string());

    let repo_for_commit = project.repo_path.clone();
    let commit_hash =
        tokio::task::spawn_blocking(move || git_manager::get_current_commit(&repo_for_commit))
            .await
            .map_err(|e| format!("Task error: {e}"))?
            .ok();

    let now = db::now_unix();
    let mut node = DecisionNode {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        parent_id: Some(merged_node.id.clone()),
        label: match &validation_plan {
            Ok(plan) => plan.label.clone(),
            Err(_) => "Validate merged build".to_string(),
        },
        prompt: match &validation_plan {
            Ok(plan) => plan.prompt.clone(),
            Err(err) => format!(
                "Automatic validation could not determine a safe build command for this repository. {err}"
            ),
        },
        branch_name,
        worktree_path: None,
        commit_hash,
        status: match &validation_plan {
            Ok(_) => NodeStatus::Pending,
            Err(_) => NodeStatus::Failed,
        },
        exit_code: match &validation_plan {
            Ok(_) => None,
            Err(_) => Some(1),
        },
        node_type: Some("validation".to_string()),
        agent_type_override: None,
        scheduled_at: None,
        started_at: None,
        created_at: now,
        updated_at: now,
    };

    let db_insert = state.db.clone();
    let node_clone = node.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db_insert
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        db::node_create(&conn, &node_clone).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if let Ok(plan) = validation_plan {
        node = run_validation_node_with_plan(state, app, project, node, plan).await?;
    }

    Ok(node)
}

async fn run_validation_node_with_plan(
    state: &AppState,
    app: &AppHandle,
    project: &Project,
    mut node: DecisionNode,
    plan: validation::ValidationPlan,
) -> Result<DecisionNode, String> {
    let repo_for_branch = project.repo_path.clone();
    node.branch_name =
        tokio::task::spawn_blocking(move || git_manager::get_default_branch(&repo_for_branch))
            .await
            .map_err(|e| format!("Task error: {e}"))?
            .unwrap_or_else(|_| "main".to_string());

    let repo_for_commit = project.repo_path.clone();
    node.commit_hash =
        tokio::task::spawn_blocking(move || git_manager::get_current_commit(&repo_for_commit))
            .await
            .map_err(|e| format!("Task error: {e}"))?
            .ok();
    node.worktree_path = None;
    node.prompt = plan.prompt;

    persist_node_runtime_fields(state.db.clone(), &node).await?;

    state.pty.clear_session_artifacts(&node.id);
    state
        .pty
        .spawn_session(
            &node.id,
            &project.id,
            &node.id,
            &plan.execution.program,
            &plan.execution.args,
            &project.repo_path,
            plan.execution.stdin_injection.as_deref(),
            plan.execution.auto_responses,
            false,
            state.db.clone(),
            app.clone(),
        )
        .map_err(|e| format!("Failed to spawn validation session: {e}"))?;

    node.status = NodeStatus::Running;
    Ok(node)
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

    let settings = get_settings().await.ok();
    let default_execution_agent = settings
        .as_ref()
        .and_then(|value| value.execution_agent.clone())
        .unwrap_or_else(|| project.agent_type.clone());
    ensure_provider_ready(&default_execution_agent, "execution").await?;

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
        agent_type_override: None,
        scheduled_at: None,
        started_at: None,
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
    let effective_agent = effective_node_agent(&node, &project, settings.as_ref());
    let effective_config = resolve_node_agent_config(&effective_agent, &project, settings.as_ref());
    let exec_model = settings.as_ref().and_then(|s| s.execution_model.as_deref());
    let execution = agent_templates::build_shell_command(
        &effective_agent,
        &project.prompt,
        &effective_config,
        None,
        node.node_type.as_deref(),
        exec_model,
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
                    true,
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
                    sdk.stdin_injection.as_deref(),
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
        agent_type_override: None,
        scheduled_at: None,
        started_at: None,
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
    if !["task", "decision", "agent", "merge", "final", "validation"].contains(&node_type.as_str())
    {
        return Err(format!(
            "Invalid structural node type: {node_type}. Must be task, decision, agent, merge, final, or validation"
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
        agent_type_override: None,
        scheduled_at: None,
        started_at: None,
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
    app: AppHandle,
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
            let commit_message = git_manager::agent_commit_message(
                node.node_type.as_deref(),
                &node.label,
                &node.prompt,
            );
            tokio::task::spawn_blocking(move || {
                match git_manager::auto_commit_worktree_with_message(&wt, &commit_message) {
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
        let resolution_config = resolve_effective_agent_config(
            &project.agent_type,
            &project.type_config,
            resolution_settings.as_ref(),
        );
        let resolution_model = agent_config_model(&resolution_config).or_else(|| {
            resolution_settings
                .as_ref()
                .and_then(|settings| settings.execution_model.clone())
        });

        let resolution = resolve_merge_conflicts(
            &project.agent_type,
            &repo_for_resolve,
            &conflicts,
            resolution_model.as_deref(),
            agent_config_extra_args(&resolution_config),
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

        let repo = project.repo_path.clone();
        tokio::task::spawn_blocking(move || git_manager::cleanup_crongen_worktrees(&repo))
            .await
            .map_err(|e| format!("Task error: {e}"))?
            .map_err(|e| format!("{e}"))?;

        log::info!("Merged node {} branch {}", node.id, node.branch_name);

        if let Err(err) =
            create_post_merge_validation_node(state.inner(), &app, &node, &project).await
        {
            log::warn!(
                "Merged node {} but failed to create post-merge validation: {}",
                node.id,
                err
            );
        }
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

    let db4 = db2.clone();
    let subtree_root = node_id.clone();
    let nodes = tokio::task::spawn_blocking(move || {
        let conn = db4.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_subtree(&conn, &subtree_root).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    // 2. Remove worktrees from this node and all descendants
    let repo = project.repo_path.clone();
    let worktree_paths: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.worktree_path.clone())
        .collect();
    tokio::task::spawn_blocking(move || git_manager::cleanup_worktrees(&repo, &worktree_paths))
        .await
        .map_err(|e| format!("Task error: {e}"))?
        .map_err(|e| format!("{e}"))?;

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
pub async fn delete_session(
    state: State<'_, AppState>,
    session_root_id: String,
) -> Result<Vec<String>, String> {
    let db = state.db.clone();
    let sid = session_root_id.clone();

    let (root, project, nodes) = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let root = db::node_get_by_id(&conn, &sid).map_err(|e| format!("{e}"))?;
        if root.parent_id.is_some() {
            return Err("Only complete sessions can be deleted from the session list.".to_string());
        }

        let project = db::project_get_by_id(&conn, &root.project_id).map_err(|e| format!("{e}"))?;
        let nodes = db::node_get_subtree(&conn, &root.id).map_err(|e| format!("{e}"))?;
        Ok((root, project, nodes))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if matches!(
        state
            .orchestrator
            .get_status(&session_root_id)
            .await
            .map(|status| status.state),
        Some(OrchestratorState::Running | OrchestratorState::WaitingUser)
    ) {
        return Err(
            "Stop or finish the active orchestrator before deleting this session.".to_string(),
        );
    }

    if nodes
        .iter()
        .any(|node| node_has_active_runtime(state.inner(), node))
    {
        return Err(
            "Stop active agent or terminal sessions before deleting this session.".to_string(),
        );
    }

    let repo = project.repo_path.clone();
    let worktree_paths: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.worktree_path.clone())
        .collect();
    tokio::task::spawn_blocking(move || git_manager::cleanup_worktrees(&repo, &worktree_paths))
        .await
        .map_err(|e| format!("Task error: {e}"))?
        .map_err(|e| format!("{e}"))?;

    let db = state.db.clone();
    let sid = session_root_id.clone();
    let deleted_ids = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_delete_branch(&conn, &sid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    for node_id in &deleted_ids {
        state.pty.clear_session_artifacts(node_id);
        state.sdk.clear_session_artifacts(node_id);
        let manual_session_id = manual_terminal_session_id(node_id);
        state.pty.clear_session_artifacts(&manual_session_id);
        state.sdk.clear_session_artifacts(&manual_session_id);
    }

    log::info!(
        "Deleted session {} ({}): {} nodes removed",
        root.id,
        root.label,
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
        agent_type_override: None,
        scheduled_at: None,
        started_at: None,
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

    // 3. Ensure the repo path is a git repo
    let repo_path = project.repo_path.clone();
    tokio::task::spawn_blocking(move || {
        git_manager::ensure_git_repo(&repo_path).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let is_validation = node.node_type.as_deref() == Some("validation");
    if !is_validation {
        let settings = get_settings().await.ok();
        let effective_agent = effective_node_agent(&node, &project, settings.as_ref());
        ensure_provider_ready(&effective_agent, "execution").await?;
    } else {
        let repo_for_validation = project.repo_path.clone();
        let plan = tokio::task::spawn_blocking(move || {
            validation::build_validation_plan(&repo_for_validation)
        })
        .await
        .map_err(|e| format!("Task error: {e}"))??;
        return run_validation_node_with_plan(state.inner(), &app, &project, node, plan).await;
    }

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
    let settings = get_settings().await.ok();
    let effective_agent = effective_node_agent(&node, &project, settings.as_ref());
    let effective_config = resolve_node_agent_config(&effective_agent, &project, settings.as_ref());
    let exec_model2 = settings.as_ref().and_then(|s| s.execution_model.as_deref());
    let execution = agent_templates::build_shell_command(
        &effective_agent,
        &node.prompt,
        &effective_config,
        Some(&toon_context),
        node.node_type.as_deref(),
        exec_model2,
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
                    true,
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
                    sdk.stdin_injection.as_deref(),
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
pub async fn update_node_agent(
    state: State<'_, AppState>,
    node_id: String,
    agent_type: Option<String>,
) -> Result<DecisionNode, String> {
    let parsed_agent = match agent_type {
        Some(value) if value.trim().is_empty() => None,
        Some(value) => {
            let parsed = AgentType::from_str(&value)?;
            if parsed == AgentType::Custom {
                return Err("Custom shells cannot be assigned to individual nodes.".to_string());
            }
            Some(parsed)
        }
        None => None,
    };

    let db = state.db.clone();
    let nid = node_id.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let node = db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))?;
        if matches!(node.node_type.as_deref(), Some("decision" | "validation")) {
            return Err("Only agent-run nodes can have an agent override.".to_string());
        }
        db::node_update_agent_type_override(&conn, &nid, parsed_agent.as_ref())
            .map_err(|e| format!("{e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?
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

#[tauri::command]
pub async fn get_codex_model_catalog() -> Result<CodexModelCatalog, String> {
    tokio::task::spawn_blocking(load_codex_model_catalog)
        .await
        .map_err(|err| format!("Task error: {err}"))?
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
pub async fn open_node_terminal(
    state: State<'_, AppState>,
    app: AppHandle,
    node_id: String,
) -> Result<NodeTerminalSession, String> {
    let db = state.db.clone();
    let nid = node_id.clone();
    let (node, project) = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let node = db::node_get_by_id(&conn, &nid).map_err(|e| format!("{e}"))?;
        let project = db::project_get_by_id(&conn, &node.project_id).map_err(|e| format!("{e}"))?;
        Ok::<(DecisionNode, Project), String>((node, project))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let cwd = resolve_node_terminal_cwd(&node, &project);
    let session_id = manual_terminal_session_id(&node.id);
    let settings = get_settings().await.ok();
    let effective_agent = effective_node_agent(&node, &project, settings.as_ref());
    ensure_provider_ready(&effective_agent, "execution").await?;
    let effective_config = resolve_node_agent_config(&effective_agent, &project, settings.as_ref());
    let execution_model = settings.as_ref().and_then(|s| s.execution_model.as_deref());
    let effective_model =
        agent_config_model(&effective_config).or_else(|| execution_model.map(ToString::to_string));

    if !state.pty.has_session(&session_id) {
        state.pty.clear_session_artifacts(&session_id);
        let shell = agent_templates::build_interactive_terminal_command(
            &effective_agent,
            &effective_config,
            execution_model,
        );
        state
            .pty
            .spawn_detached_shell_session(
                &session_id,
                &project.id,
                &shell.program,
                &shell.args,
                &cwd,
                app,
            )
            .map_err(|e| format!("Failed to open agent terminal: {e}"))?;
    }

    Ok(NodeTerminalSession {
        session_id,
        cwd,
        agent_label: agent_label(&effective_agent).to_string(),
        model: effective_model,
    })
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

#[tauri::command]
pub async fn stop_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let stop_result = state
        .pty
        .stop_session(&session_id)
        .or_else(|_| state.sdk.stop_session(&session_id));
    stop_result.map_err(|e| format!("Stop error: {e}"))?;
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
    let (project, nodes) = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let root = db::node_get_by_id(&conn, &root_id).map_err(|e| format!("{e}"))?;
        let project = db::project_get_by_id(&conn, &root.project_id).map_err(|e| format!("{e}"))?;
        let nodes = db::node_get_subtree(&conn, &root_id).map_err(|e| format!("{e}"))?;
        Ok::<(Project, Vec<DecisionNode>), String>((project, nodes))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let settings = get_settings().await.ok();
    let mut required_agents = Vec::<AgentType>::new();
    for node in nodes {
        if node.status != NodeStatus::Pending {
            continue;
        }
        if matches!(node.node_type.as_deref(), Some("decision" | "validation")) {
            continue;
        }
        let agent = effective_node_agent(&node, &project, settings.as_ref());
        if !required_agents.contains(&agent) {
            required_agents.push(agent);
        }
    }

    for agent in required_agents {
        ensure_provider_ready(&agent, "execution").await?;
    }

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
    extra_args: &[String],
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

    let invocation =
        build_merge_resolution_invocation(agent_type, repo_path, &prompt, model, extra_args)?;
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
            match session_backend.as_deref() {
                Some("pty") => "The PTY session is still active and streaming output. If the agent is idle at a prompt, open the Session tab and use Send Enter.".to_string(),
                _ => format!(
                    "The {} session is still active and streaming output.",
                    session_backend.as_deref().unwrap_or("agent")
                ),
            }
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

    let node = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_get_by_id(&conn, &nid).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if active_session_backend(state.pty.as_ref(), state.sdk.as_ref(), &node.id).is_some() {
        return Err(
            "This node still has an active session. Validate its runtime state first or continue the live session.".to_string(),
        );
    }

    let db_project = state.db.clone();
    let pid = node.project_id.clone();
    let project = tokio::task::spawn_blocking(move || {
        let conn = db_project
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        db::project_get_by_id(&conn, &pid).map_err(|e| format!("DB error: {e}"))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    if let Some(worktree_path) = node.worktree_path.clone() {
        if std::path::Path::new(&worktree_path).exists() {
            let repo_path = project.repo_path.clone();
            let cleanup_worktree_path = worktree_path.clone();
            let cleanup_result = tokio::task::spawn_blocking(move || {
                git_manager::remove_worktree(&repo_path, &cleanup_worktree_path, true)
                    .map_err(|e| format!("{e}"))
            })
            .await
            .map_err(|e| format!("Task error: {e}"))?;

            if let Err(err) = cleanup_result {
                log::warn!(
                    "Failed to remove worktree for node {} during reset: {}",
                    node.id,
                    err
                );
            }
        }
    }

    state.pty.clear_session_artifacts(&node.id);
    state.sdk.clear_session_artifacts(&node.id);

    let db_reset = state.db.clone();
    let nid = node_id.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db_reset.lock().map_err(|e| format!("DB lock error: {e}"))?;
        conn.execute(
            "UPDATE decision_nodes
             SET status=?1, exit_code=NULL, worktree_path=NULL, commit_hash=NULL,
                 started_at=NULL, updated_at=?2
             WHERE id=?3",
            rusqlite::params![NodeStatus::Pending.as_str(), db::now_unix(), nid],
        )
        .map_err(|e| format!("DB error: {e}"))?;
        Ok::<(), String>(())
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
    let planning_config = settings_agent_config(&planning_agent, Some(&settings));
    let planning_model = planning_config
        .as_ref()
        .and_then(agent_config_model)
        .or_else(|| settings.planning_model.clone());
    let planning_extra_args = planning_config
        .as_ref()
        .map(agent_config_extra_args)
        .unwrap_or(&[]);
    let plan = plan_generator::generate_plan(
        &planning_agent,
        &prompt,
        &project_mode,
        planning_model.as_deref(),
        planning_extra_args,
        complexity_str,
        &repo_path,
    )
    .await?;

    // Linear plans are normalized into a single chain so the canvas stays straightforward.
    let plan = plan_generator::normalize_plan_for_complexity(plan, complexity_str);

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

#[tauri::command]
pub async fn generate_plan_children(
    state: State<'_, AppState>,
    project_id: String,
    parent_id: String,
    prompt: String,
    complexity: Option<String>,
) -> Result<Vec<DecisionNode>, String> {
    let settings = get_settings().await?;
    let planning_agent = resolve_planning_agent(&settings)?;
    ensure_provider_ready(&planning_agent, "planning").await?;

    let db_mode = state.db.clone();
    let pid = project_id.clone();
    let parent = parent_id.clone();
    let (project_mode, repo_path) = tokio::task::spawn_blocking(move || {
        let conn = db_mode.lock().map_err(|e| format!("DB lock: {e}"))?;
        let parent_node =
            db::node_get_by_id(&conn, &parent).map_err(|e| format!("Parent node: {e}"))?;
        if parent_node.project_id != pid {
            return Err("Parent node does not belong to the selected project.".to_string());
        }
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

    let complexity_str = complexity.as_deref().unwrap_or("branching");
    let planning_config = settings_agent_config(&planning_agent, Some(&settings));
    let planning_model = planning_config
        .as_ref()
        .and_then(agent_config_model)
        .or_else(|| settings.planning_model.clone());
    let planning_extra_args = planning_config
        .as_ref()
        .map(agent_config_extra_args)
        .unwrap_or(&[]);
    let plan = plan_generator::generate_plan(
        &planning_agent,
        &prompt,
        &project_mode,
        planning_model.as_deref(),
        planning_extra_args,
        complexity_str,
        &repo_path,
    )
    .await?;

    let plan = plan_generator::normalize_plan_for_complexity(plan, complexity_str);
    let nodes = plan_generator::plan_children_to_nodes(&plan, &project_id, &parent_id);

    if nodes.is_empty() {
        return Err("Planner did not return any child steps.".to_string());
    }

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
        "Generated {} child plan nodes under {} for project {}",
        nodes.len(),
        parent_id,
        project_id
    );
    Ok(nodes)
}

#[tauri::command]
pub async fn refine_plan(
    state: State<'_, AppState>,
    project_id: String,
    session_root_id: String,
    provider: String,
    lenses: Vec<String>,
    guidance: Option<String>,
) -> Result<Vec<DecisionNode>, String> {
    let provider = AgentType::from_str(&provider)?;
    if provider == AgentType::Custom {
        return Err("Custom providers are not supported for plan refinement.".to_string());
    }
    ensure_provider_ready(&provider, "planning").await?;

    let settings = get_settings().await?;
    let db_read = state.db.clone();
    let pid = project_id.clone();
    let sid = session_root_id.clone();
    let (project_mode, repo_path, current_nodes) = tokio::task::spawn_blocking(move || {
        let conn = db_read.lock().map_err(|e| format!("DB lock: {e}"))?;
        let project = db::project_get_by_id(&conn, &pid).map_err(|e| format!("{e}"))?;
        let root = db::node_get_by_id(&conn, &sid).map_err(|e| format!("Root node: {e}"))?;
        if root.project_id != pid || root.parent_id.is_some() {
            return Err("Refinement requires a selected session root for this project.".to_string());
        }

        let nodes = db::node_get_subtree(&conn, &sid).map_err(|e| format!("{e}"))?;
        if nodes.is_empty() {
            return Err("No flow exists to refine.".to_string());
        }
        if nodes.iter().any(|node| node.status != crate::models::NodeStatus::Pending) {
            return Err(
                "Only draft plans can be refined. Reset or create a new plan before refining executed nodes."
                    .to_string(),
            );
        }
        if nodes
            .iter()
            .any(|node| node.worktree_path.is_some() || node.commit_hash.is_some())
        {
            return Err(
                "This flow already has execution artifacts. Create a new draft before refining it."
                    .to_string(),
            );
        }

        let project_mode = if project.project_mode == "blank" {
            let roots = db::node_get_roots(&conn, &pid).map_err(|e| format!("{e}"))?;
            let has_completed = roots.iter().any(|r| {
                r.status == crate::models::NodeStatus::Completed
                    || r.status == crate::models::NodeStatus::Merged
            });
            if has_completed {
                "existing".to_string()
            } else {
                project.project_mode
            }
        } else {
            project.project_mode
        };

        Ok::<(String, String, Vec<DecisionNode>), String>((project_mode, project.repo_path, nodes))
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    let provider_config = settings_agent_config(&provider, Some(&settings));
    let planning_model = provider_config
        .as_ref()
        .and_then(agent_config_model)
        .or_else(|| {
            if settings.planning_agent.as_ref() == Some(&provider) {
                settings.planning_model.clone()
            } else {
                None
            }
        });
    let planning_extra_args = provider_config
        .as_ref()
        .map(agent_config_extra_args)
        .unwrap_or(&[]);

    let plan = plan_generator::refine_plan(
        &provider,
        &current_nodes,
        &project_mode,
        &lenses,
        guidance.as_deref(),
        planning_model.as_deref(),
        planning_extra_args,
        &repo_path,
    )
    .await?;

    let refined_nodes = plan_generator::plan_to_nodes(&plan, &project_id);
    if refined_nodes.is_empty() {
        return Err("Refinement returned an empty flow.".to_string());
    }

    let db_write = state.db.clone();
    let old_root = session_root_id.clone();
    let nodes_clone = refined_nodes.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db_write.lock().map_err(|e| format!("DB lock error: {e}"))?;
        db::node_delete_branch(&conn, &old_root).map_err(|e| format!("DB delete: {e}"))?;
        for node in &nodes_clone {
            db::node_create(&conn, node).map_err(|e| format!("DB insert: {e}"))?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    log::info!(
        "Refined plan for project {} with {} into {} nodes",
        project_id,
        provider.as_str(),
        refined_nodes.len()
    );
    Ok(refined_nodes)
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
) -> Result<FeatureBranchResult, String> {
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| format!("DB lock error: {e}"))?;
        let node = db::node_get_by_id(&conn, &node_id).map_err(|e| format!("{e}"))?;
        let project = db::project_get_by_id(&conn, &node.project_id).map_err(|e| format!("{e}"))?;

        let mut commit = node
            .commit_hash
            .clone()
            .ok_or_else(|| "Node has no commit hash — run the session first".to_string())?;

        if let Some(worktree_path) = node.worktree_path.as_deref() {
            if std::path::Path::new(worktree_path).exists() {
                let commit_message = git_manager::agent_commit_message(
                    node.node_type.as_deref(),
                    &node.label,
                    &node.prompt,
                );
                git_manager::auto_commit_worktree_with_message(worktree_path, &commit_message)
                    .map_err(|e| format!("{e}"))?;
                commit =
                    git_manager::get_current_commit(worktree_path).map_err(|e| format!("{e}"))?;
                db::node_update_commit(&conn, &node_id, &commit).map_err(|e| format!("{e}"))?;
            }
        }

        let created_branch =
            git_manager::create_branch_at_and_checkout(&project.repo_path, &branch_name, &commit)
                .map_err(|e| format!("{e}"))?;

        git_manager::cleanup_crongen_worktrees(&project.repo_path).map_err(|e| format!("{e}"))?;

        Ok(FeatureBranchResult {
            branch_name: created_branch,
            commit_hash: commit,
        })
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
        build_merge_resolution_invocation, classify_codex_login_status, effective_node_agent,
        parse_claude_auth_logged_in, parse_codex_model_catalog, resolve_node_agent_config,
        role_requires_provider_validation,
    };
    use crate::models::{
        AgentCliConfigs, AgentProviderStatus, AgentType, AgentTypeConfig, AppSettings,
        ClaudeCodeConfig, CodexConfig, DecisionNode, NodeStatus, Project,
    };

    fn test_project(agent_type: AgentType, type_config: AgentTypeConfig) -> Project {
        Project {
            id: "project".to_string(),
            name: "Project".to_string(),
            prompt: "Prompt".to_string(),
            shell: "zsh".to_string(),
            repo_path: "/tmp/project".to_string(),
            is_active: true,
            agent_type,
            type_config,
            project_mode: "existing".to_string(),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn test_node(agent_type_override: Option<AgentType>) -> DecisionNode {
        DecisionNode {
            id: "node".to_string(),
            project_id: "project".to_string(),
            parent_id: None,
            label: "Node".to_string(),
            prompt: "Run this".to_string(),
            branch_name: "pending/node".to_string(),
            worktree_path: None,
            commit_hash: None,
            status: NodeStatus::Pending,
            exit_code: None,
            node_type: Some("agent".to_string()),
            agent_type_override,
            scheduled_at: None,
            started_at: None,
            created_at: 1,
            updated_at: 1,
        }
    }

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
    fn node_agent_override_wins_over_default_and_project() {
        let mut settings = AppSettings::default();
        settings.execution_agent = Some(AgentType::Gemini);
        let project = test_project(
            AgentType::Codex,
            AgentTypeConfig::Codex(CodexConfig::default()),
        );
        let node = test_node(Some(AgentType::ClaudeCode));

        assert_eq!(
            effective_node_agent(&node, &project, Some(&settings)),
            AgentType::ClaudeCode
        );
    }

    #[test]
    fn default_execution_agent_wins_when_node_has_no_override() {
        let mut settings = AppSettings::default();
        settings.execution_agent = Some(AgentType::Gemini);
        let project = test_project(
            AgentType::Codex,
            AgentTypeConfig::Codex(CodexConfig::default()),
        );
        let node = test_node(None);

        assert_eq!(
            effective_node_agent(&node, &project, Some(&settings)),
            AgentType::Gemini
        );
    }

    #[test]
    fn overridden_node_uses_matching_settings_config_not_project_config() {
        let mut settings = AppSettings::default();
        settings.agent_configs = AgentCliConfigs {
            claude_code: Some(ClaudeCodeConfig {
                model: Some("settings-claude".to_string()),
                ..ClaudeCodeConfig::default()
            }),
            codex: None,
            gemini: None,
        };
        let project = test_project(
            AgentType::Codex,
            AgentTypeConfig::Codex(CodexConfig {
                model: Some("project-codex".to_string()),
                ..CodexConfig::default()
            }),
        );

        let config = resolve_node_agent_config(&AgentType::ClaudeCode, &project, Some(&settings));

        match config {
            AgentTypeConfig::ClaudeCode(config) => {
                assert_eq!(config.model.as_deref(), Some("settings-claude"));
            }
            other => panic!("expected Claude config, got {other:?}"),
        }
    }

    #[test]
    fn unrelated_project_config_is_not_reused_for_overridden_agent() {
        let project = test_project(
            AgentType::Codex,
            AgentTypeConfig::Codex(CodexConfig {
                model: Some("project-codex".to_string()),
                ..CodexConfig::default()
            }),
        );

        let config = resolve_node_agent_config(&AgentType::ClaudeCode, &project, None);

        match config {
            AgentTypeConfig::ClaudeCode(config) => assert_eq!(config.model, None),
            other => panic!("expected default Claude config, got {other:?}"),
        }
    }

    #[test]
    fn codex_merge_resolution_uses_output_file() {
        let invocation = build_merge_resolution_invocation(
            &AgentType::Codex,
            "/tmp",
            "Resolve the merge",
            Some("gpt-5"),
            &[],
        )
        .expect("codex invocation");

        assert_eq!(invocation.program, "codex");
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "--output-last-message"));
        assert!(invocation.output_file.is_some());
    }

    #[test]
    fn codex_merge_resolution_clamps_reasoning_for_fast_model() {
        let invocation = build_merge_resolution_invocation(
            &AgentType::Codex,
            "/tmp",
            "Resolve the merge",
            Some("gpt-5-codex-mini"),
            &[],
        )
        .expect("codex invocation");

        assert!(invocation.args.iter().any(|arg| arg == "-c"));
        assert!(invocation
            .args
            .iter()
            .any(|arg| arg == "model_reasoning_effort=\"medium\""));
    }

    #[test]
    fn gemini_merge_resolution_uses_headless_json_prompt() {
        let invocation = build_merge_resolution_invocation(
            &AgentType::Gemini,
            "/tmp",
            "Resolve the merge",
            Some("gemini-2.5-pro"),
            &["--include-directories".to_string(), "../shared".to_string()],
        )
        .expect("gemini invocation");

        assert_eq!(invocation.program, "gemini");
        assert_eq!(invocation.output_file, None);
        assert!(invocation.args.iter().any(|arg| arg == "--yolo"));
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--output-format", "json"]));
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "gemini-2.5-pro"]));
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--include-directories", "../shared"]));
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--prompt", "Resolve the merge"]));
    }

    #[test]
    fn parses_codex_model_catalog_and_filters_hidden_entries() {
        let raw = r#"
        {
          "fetched_at": "2026-03-23T18:39:04.683298Z",
          "client_version": "0.116.0",
          "models": [
            {
              "slug": "gpt-5.4-mini",
              "display_name": "GPT-5.4 Mini",
              "description": "Fast model",
              "default_reasoning_level": "medium",
              "supported_reasoning_levels": [{ "effort": "medium", "description": "Balanced" }],
              "visibility": "list",
              "priority": 2
            },
            {
              "slug": "gpt-5.4",
              "display_name": "GPT-5.4",
              "description": "Frontier model",
              "default_reasoning_level": "high",
              "supported_reasoning_levels": [{ "effort": "high", "description": "Deep" }],
              "visibility": "list",
              "priority": 0
            },
            {
              "slug": "hidden-model",
              "display_name": "Hidden",
              "visibility": "hidden",
              "priority": 1
            }
          ]
        }
        "#;

        let catalog = parse_codex_model_catalog(raw).expect("catalog should parse");

        assert_eq!(catalog.source, "codex_models_cache");
        assert_eq!(catalog.client_version.as_deref(), Some("0.116.0"));
        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.models[0].slug, "gpt-5.4");
        assert_eq!(catalog.models[1].slug, "gpt-5.4-mini");
        assert_eq!(
            catalog.models[0].supported_reasoning_levels[0].effort,
            "high"
        );
    }
}
