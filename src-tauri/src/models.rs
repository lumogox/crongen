use serde::{Deserialize, Serialize};

// ─── Agent Types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    ClaudeCode,
    Codex,
    Gemini,
    Custom,
}

impl AgentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentType::ClaudeCode => "claude_code",
            AgentType::Codex => "codex",
            AgentType::Gemini => "gemini",
            AgentType::Custom => "custom",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "claude_code" => Ok(AgentType::ClaudeCode),
            "codex" => Ok(AgentType::Codex),
            "gemini" => Ok(AgentType::Gemini),
            "custom" => Ok(AgentType::Custom),
            other => Err(format!("Unknown agent type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProviderStatus {
    Ready,
    MissingCli,
    NeedsLogin,
    ComingSoon,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProviderReadiness {
    pub agent_type: AgentType,
    pub status: AgentProviderStatus,
    pub detail: Option<String>,
    pub ready: bool,
    pub supports_planning: bool,
    pub supports_execution: bool,
    pub coming_soon: bool,
}

impl AgentProviderReadiness {
    pub fn new(
        agent_type: AgentType,
        status: AgentProviderStatus,
        detail: Option<String>,
        supports_planning: bool,
        supports_execution: bool,
        coming_soon: bool,
    ) -> Self {
        let ready = status == AgentProviderStatus::Ready;
        Self {
            agent_type,
            status,
            detail,
            ready,
            supports_planning,
            supports_execution,
            coming_soon,
        }
    }
}

// ─── Agent Type Configs ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentTypeConfig {
    ClaudeCode(ClaudeCodeConfig),
    Codex(CodexConfig),
    Gemini(GeminiConfig),
    Custom(CustomConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeCodeConfig {
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub max_budget_usd: Option<f64>,
    pub allowed_tools: Option<String>,
    pub disallowed_tools: Option<String>,
    pub append_system_prompt: Option<String>,
    #[serde(default = "default_true")]
    pub dangerously_skip_permissions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodexConfig {
    pub model: Option<String>,
    pub sandbox: Option<String>,       // "full", "network-only", "none"
    pub approval_mode: Option<String>, // "full-auto", "suggest", "auto-edit"
    pub skip_git_check: bool,
    pub json_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeminiConfig {
    pub model: Option<String>,
    pub sandbox: Option<String>,
    #[serde(default = "default_true")]
    pub yolo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomConfig {
    pub shell: Option<String>, // e.g. "bash", "zsh", "pwsh"
}

fn default_true() -> bool {
    true
}

// ─── Project ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub prompt: String, // Used as "description" in the UI
    pub shell: String,  // The resolved executable (e.g. "claude", "codex", "bash")
    pub repo_path: String,
    pub is_active: bool,
    pub agent_type: AgentType,
    pub type_config: AgentTypeConfig,
    pub project_mode: String, // "blank" (scaffold new) or "existing" (add to existing)
    pub created_at: i64,
    pub updated_at: i64,
}

// ─── Decision Node ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Merged,
}

impl NodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeStatus::Pending => "pending",
            NodeStatus::Running => "running",
            NodeStatus::Paused => "paused",
            NodeStatus::Completed => "completed",
            NodeStatus::Failed => "failed",
            NodeStatus::Merged => "merged",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(NodeStatus::Pending),
            "running" => Ok(NodeStatus::Running),
            "paused" => Ok(NodeStatus::Paused),
            "completed" => Ok(NodeStatus::Completed),
            "failed" => Ok(NodeStatus::Failed),
            "merged" => Ok(NodeStatus::Merged),
            other => Err(format!("Unknown node status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionNode {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub label: String,
    pub prompt: String,
    pub branch_name: String,
    pub worktree_path: Option<String>,
    pub commit_hash: Option<String>,
    pub status: NodeStatus,
    pub exit_code: Option<i32>,
    pub node_type: Option<String>, // "task", "agent", "decision", "merge", "final"
    pub scheduled_at: Option<String>, // ISO 8601 datetime — only meaningful on root (session) nodes
    pub created_at: i64,
    pub updated_at: i64,
}

// ─── Shell Execution (output of agent_templates) ───────────────

/// A pattern→response pair for auto-answering interactive prompts.
/// When `pattern` appears in PTY output, `response` is injected via stdin.
/// Each auto-response fires at most once per session.
#[derive(Debug, Clone)]
pub struct AutoResponse {
    pub pattern: String,
    pub response: String,
    /// Milliseconds to wait before sending `response` (lets TUI settle after match).
    pub delay_ms: u64,
    /// If true, send `\r` (Enter) after `response` with a 200ms gap.
    /// Use for prompt text that needs to be "typed then submitted."
    pub submit: bool,
}

#[derive(Debug, Clone)]
pub struct ShellExecution {
    pub program: String,
    pub args: Vec<String>,
    pub stdin_injection: Option<String>,
    pub auto_responses: Vec<AutoResponse>,
}

// ─── SDK Execution (headless `claude -p` mode) ─────────────────

#[derive(Debug, Clone)]
pub struct SdkExecution {
    pub program: String,
    pub args: Vec<String>,
}

/// Determines how an agent process is spawned and managed.
#[derive(Debug, Clone)]
pub enum ExecutionMode {
    /// Full PTY with xterm.js rendering (Codex, Gemini, Custom).
    Pty(ShellExecution),
    /// Headless subprocess with structured JSON output (Claude Code).
    Sdk(SdkExecution),
}

/// Tauri event payload for SDK JSON line output.
#[derive(Clone, serde::Serialize)]
pub struct SdkOutputPayload {
    pub session_id: String,
    pub data: String, // raw JSON line
}

// ─── Orchestrator Types ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorMode {
    Auto,
    Supervised,
}

impl OrchestratorMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrchestratorMode::Auto => "auto",
            OrchestratorMode::Supervised => "supervised",
        }
    }
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(OrchestratorMode::Auto),
            "supervised" => Ok(OrchestratorMode::Supervised),
            other => Err(format!("Unknown orchestrator mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorState {
    Idle,
    Running,
    WaitingUser,
    Complete,
    Failed,
}

impl OrchestratorState {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrchestratorState::Idle => "idle",
            OrchestratorState::Running => "running",
            OrchestratorState::WaitingUser => "waiting_user",
            OrchestratorState::Complete => "complete",
            OrchestratorState::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorStatus {
    pub session_id: String,
    pub state: OrchestratorState,
    pub mode: OrchestratorMode,
    pub current_node_id: Option<String>,
    pub completed_count: usize,
    pub total_count: usize,
    pub pending_decision: Option<PendingDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDecision {
    pub node_id: String,
    pub label: String,
    pub prompt: String,
    pub options: Vec<DecisionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    pub node_id: String,
    pub label: String,
    pub prompt: String,
}

// ─── App Settings ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub debug_mode: bool,
    #[serde(default)]
    pub agent_setup_seen: bool,
    #[serde(default)]
    pub planning_agent: Option<AgentType>,
    #[serde(default)]
    pub execution_agent: Option<AgentType>,
    #[serde(default)]
    pub planning_model: Option<String>,
    #[serde(default)]
    pub execution_model: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            debug_mode: false,
            agent_setup_seen: false,
            planning_agent: None,
            execution_agent: None,
            planning_model: None,
            execution_model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRuntimeValidation {
    pub node: DecisionNode,
    pub session_active: bool,
    pub session_backend: Option<String>,
    pub reconciled: bool,
    pub message: String,
}

/// Completion event emitted by PTY/SDK managers when a session finishes.
#[derive(Debug, Clone)]
pub struct SessionCompletion {
    pub node_id: String,
    pub exit_code: Option<i32>,
}

/// Tauri event payloads for orchestrator events.
#[derive(Clone, Serialize)]
pub struct OrchestratorProgressPayload {
    pub session_id: String,
    pub node_id: String,
    pub status: String,
    pub completed_count: usize,
    pub total_count: usize,
}

#[derive(Clone, Serialize)]
pub struct OrchestratorDecisionPayload {
    pub session_id: String,
    pub decision: PendingDecision,
}

#[derive(Clone, Serialize)]
pub struct OrchestratorCompletePayload {
    pub session_id: String,
    pub success: bool,
}
