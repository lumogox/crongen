use crate::models::{
    AgentType, AgentTypeConfig, AutoResponse, ClaudeCodeConfig, CodexConfig, ExecutionMode,
    GeminiConfig, SdkExecution, ShellExecution,
};

/// Builds the execution mode for a given agent type and prompt.
///
/// Claude Code and Codex use structured SDK mode.
/// Gemini and custom agents stay on the PTY path.
///
/// When `context` is provided (TOON-formatted execution context), it is
/// prepended to the prompt/injection so the agent knows its place in the tree.
pub fn build_shell_command(
    agent_type: &AgentType,
    prompt: &str,
    config: &AgentTypeConfig,
    context: Option<&str>,
    node_type: Option<&str>,
    default_model: Option<&str>,
) -> ExecutionMode {
    // Build the context-enriched prompt
    let mut effective_prompt = match context {
        Some(ctx) => format!("{ctx}{prompt}"),
        None => prompt.to_string(),
    };

    // For merge nodes: instruct the agent to pick a winner, merge its branch, and write DECISION.md
    if node_type == Some("merge") {
        effective_prompt.push_str(
            "\n\nIMPORTANT — MERGE PROCEDURE (follow these steps exactly):\n\
             1. Review the sibling_diffs above to compare each approach.\n\
             2. Decide which approach is best (or combine the best parts).\n\
             3. Run `git merge <winning-branch-name> --no-edit` to bring the winning code into your worktree. \
                The branch names are listed in sibling_diffs above.\n\
             4. If there are merge conflicts, resolve them.\n\
             5. Write a DECISION.md at the repo root containing: (a) which approach you chose, \
                (b) why, (c) key files changed.\n\
             6. Commit all changes including DECISION.md.",
        );
    }

    match (agent_type, config) {
        (AgentType::ClaudeCode, AgentTypeConfig::ClaudeCode(cfg)) => {
            build_claude_code_command(&effective_prompt, cfg, default_model)
        }
        (AgentType::Codex, AgentTypeConfig::Codex(cfg)) => {
            build_codex_exec_command(&effective_prompt, cfg, default_model)
        }
        (AgentType::Gemini, AgentTypeConfig::Gemini(cfg)) => {
            ExecutionMode::Pty(build_gemini_command(&effective_prompt, cfg, default_model))
        }
        (AgentType::Custom, AgentTypeConfig::Custom(cfg)) => {
            ExecutionMode::Pty(build_custom_command(&effective_prompt, cfg))
        }
        // Fallback: if type/config mismatch, treat as custom with bash
        _ => ExecutionMode::Pty(ShellExecution {
            program: "bash".to_string(),
            args: vec![],
            stdin_injection: Some(effective_prompt),
            auto_responses: vec![],
        }),
    }
}

/// Builds an interactive PTY command for the configured agent without sending
/// an initial task prompt. This is used when the user wants to drop into the
/// same coding agent inside a node's workspace.
pub fn build_interactive_terminal_command(
    agent_type: &AgentType,
    config: &AgentTypeConfig,
    default_model: Option<&str>,
) -> ShellExecution {
    match (agent_type, config) {
        (AgentType::ClaudeCode, AgentTypeConfig::ClaudeCode(cfg)) => {
            build_claude_interactive_command(cfg, default_model)
        }
        (AgentType::Codex, AgentTypeConfig::Codex(cfg)) => {
            build_codex_interactive_command(cfg, default_model)
        }
        (AgentType::Gemini, AgentTypeConfig::Gemini(cfg)) => {
            build_gemini_interactive_command(cfg, default_model)
        }
        (AgentType::Custom, AgentTypeConfig::Custom(cfg)) => ShellExecution {
            program: cfg.shell.as_deref().unwrap_or("bash").to_string(),
            args: vec![],
            stdin_injection: None,
            auto_responses: vec![],
        },
        _ => ShellExecution {
            program: default_shell_for_type(agent_type).to_string(),
            args: vec![],
            stdin_injection: None,
            auto_responses: vec![],
        },
    }
}

fn build_claude_code_command(
    prompt: &str,
    cfg: &ClaudeCodeConfig,
    default_model: Option<&str>,
) -> ExecutionMode {
    let mut args = vec![
        "-p".to_string(),
        prompt.to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ];

    if cfg.dangerously_skip_permissions {
        args.push("--dangerously-skip-permissions".to_string());
    }

    // Per-agent model takes priority, then global execution_model from settings
    let model = cfg.model.as_deref().or(default_model);
    if let Some(m) = model {
        args.push("--model".to_string());
        args.push(m.to_string());
    }

    if let Some(max_turns) = cfg.max_turns {
        args.push("--max-turns".to_string());
        args.push(max_turns.to_string());
    }

    if let Some(budget) = cfg.max_budget_usd {
        args.push("--max-cost".to_string());
        args.push(format!("{budget:.2}"));
    }

    if let Some(ref tools) = cfg.allowed_tools {
        for tool in tools.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
            args.push("--allowedTools".to_string());
            args.push(tool.to_string());
        }
    }

    if let Some(ref tools) = cfg.disallowed_tools {
        for tool in tools.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
            args.push("--disallowedTools".to_string());
            args.push(tool.to_string());
        }
    }

    if let Some(ref sys_prompt) = cfg.append_system_prompt {
        args.push("--append-system-prompt".to_string());
        args.push(sys_prompt.clone());
    }

    ExecutionMode::Sdk(SdkExecution {
        program: "claude".to_string(),
        args,
        stdin_injection: None,
    })
}

fn build_claude_interactive_command(
    cfg: &ClaudeCodeConfig,
    default_model: Option<&str>,
) -> ShellExecution {
    let mut args = Vec::new();

    if cfg.dangerously_skip_permissions {
        args.push("--dangerously-skip-permissions".to_string());
    }

    let model = cfg.model.as_deref().or(default_model);
    if let Some(m) = model {
        args.push("--model".to_string());
        args.push(m.to_string());
    }

    if let Some(budget) = cfg.max_budget_usd {
        args.push("--max-budget-usd".to_string());
        args.push(format!("{budget:.2}"));
    }

    if let Some(ref tools) = cfg.allowed_tools {
        for tool in tools.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
            args.push("--allowedTools".to_string());
            args.push(tool.to_string());
        }
    }

    if let Some(ref tools) = cfg.disallowed_tools {
        for tool in tools.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()) {
            args.push("--disallowedTools".to_string());
            args.push(tool.to_string());
        }
    }

    if let Some(ref sys_prompt) = cfg.append_system_prompt {
        args.push("--append-system-prompt".to_string());
        args.push(sys_prompt.clone());
    }

    ShellExecution {
        program: "claude".to_string(),
        args,
        stdin_injection: None,
        auto_responses: vec![],
    }
}

fn build_codex_exec_command(
    prompt: &str,
    cfg: &CodexConfig,
    default_model: Option<&str>,
) -> ExecutionMode {
    let mut args = Vec::new();

    args.push("exec".to_string());
    args.push("--json".to_string());

    if let Some(flag) = codex_approval_flag(cfg.approval_mode.as_deref()) {
        args.push(flag.to_string());
    }

    // Optional flags
    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());

        if matches!(
            model,
            "gpt-5-codex-mini" | "codex-1p-mini-q-20251105-ev3"
        ) {
            args.push("-c".to_string());
            args.push("model_reasoning_effort=\"medium\"".to_string());
        }
    }

    if let Some(ref sandbox) = cfg.sandbox {
        args.push("--sandbox".to_string());
        args.push(sandbox.clone());
    }

    if cfg.skip_git_check {
        args.push("--skip-git-repo-check".to_string());
    }

    if cfg.json_output {
        // Already enforced above; keep config field harmlessly compatible.
    }

    // Read the full prompt from stdin to avoid shell/TTY timing issues and to
    // keep large execution-context payloads off the argv boundary.
    args.push("-".to_string());

    ExecutionMode::Sdk(SdkExecution {
        program: "codex".to_string(),
        args,
        stdin_injection: Some(prompt.to_string()),
    })
}

fn build_codex_interactive_command(
    cfg: &CodexConfig,
    default_model: Option<&str>,
) -> ShellExecution {
    let mut args = Vec::new();

    if let Some(flag) = codex_approval_flag(cfg.approval_mode.as_deref()) {
        args.push(flag.to_string());
    }

    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());

        if matches!(
            model,
            "gpt-5-codex-mini" | "codex-1p-mini-q-20251105-ev3"
        ) {
            args.push("-c".to_string());
            args.push("model_reasoning_effort=\"medium\"".to_string());
        }
    }

    if let Some(ref sandbox) = cfg.sandbox {
        args.push("--sandbox".to_string());
        args.push(sandbox.clone());
    }

    if cfg.skip_git_check {
        args.push("--skip-git-repo-check".to_string());
    }

    ShellExecution {
        program: "codex".to_string(),
        args,
        stdin_injection: None,
        auto_responses: vec![],
    }
}

fn codex_approval_flag(mode: Option<&str>) -> Option<&'static str> {
    match mode.unwrap_or("full-auto") {
        "full-auto" => Some("--full-auto"),
        // Legacy values don't map 1:1 to the current Codex exec CLI.
        // Falling back to the CLI default keeps runtime and previews aligned.
        "suggest" | "auto-edit" | "default" | "" => None,
        _ => None,
    }
}

fn build_gemini_command(
    prompt: &str,
    cfg: &GeminiConfig,
    default_model: Option<&str>,
) -> ShellExecution {
    let mut args = Vec::new();

    // Interactive mode: auto-approve flag so the agent doesn't block on confirmations
    if cfg.yolo {
        args.push("--yolo".to_string());
    }

    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    if let Some(ref sandbox) = cfg.sandbox {
        args.push("--sandbox".to_string());
        args.push(sandbox.clone());
    }

    ShellExecution {
        program: "gemini".to_string(),
        args,
        // No stdin_injection — Gemini's MCP server init takes variable time,
        // so a fixed delay is unreliable. Prompt is injected via auto-response
        // when "Type your message" appears (= input is actually ready).
        stdin_injection: None,
        auto_responses: vec![
            // Trust prompt: press Enter to select "Trust folder" (first option).
            // This only appears on first run in a new folder.
            AutoResponse {
                pattern: "Do you trust this folder".to_string(),
                response: "".to_string(),
                delay_ms: 100,
                submit: true,
            },
            // Input ready: inject the prompt when Gemini shows its input indicator.
            // Fires once — covers both "already trusted" and "post-trust-restart".
            AutoResponse {
                pattern: "Type your message".to_string(),
                response: prompt.to_string(),
                delay_ms: 500,
                submit: true,
            },
        ],
    }
}

fn build_gemini_interactive_command(
    cfg: &GeminiConfig,
    default_model: Option<&str>,
) -> ShellExecution {
    let mut args = Vec::new();

    if cfg.yolo {
        args.push("--yolo".to_string());
    }

    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    if let Some(ref sandbox) = cfg.sandbox {
        args.push("--sandbox".to_string());
        args.push(sandbox.clone());
    }

    ShellExecution {
        program: "gemini".to_string(),
        args,
        stdin_injection: None,
        auto_responses: vec![],
    }
}

fn build_custom_command(prompt: &str, cfg: &crate::models::CustomConfig) -> ShellExecution {
    let shell = cfg.shell.as_deref().unwrap_or("bash").to_string();

    ShellExecution {
        program: shell,
        args: vec![],
        stdin_injection: Some(prompt.to_string()),
        auto_responses: vec![],
    }
}

/// Returns the default executable name for an agent type.
pub fn default_shell_for_type(agent_type: &AgentType) -> &'static str {
    match agent_type {
        AgentType::ClaudeCode => "claude",
        AgentType::Codex => "codex",
        AgentType::Gemini => "gemini",
        AgentType::Custom => "bash",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentTypeConfig, CodexConfig};

    #[test]
    fn codex_exec_uses_stdin_json_and_repo_skip_flag() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Implement the feature",
            &AgentTypeConfig::Codex(CodexConfig {
                model: Some("gpt-5.4".to_string()),
                sandbox: Some("workspace-write".to_string()),
                approval_mode: Some("full-auto".to_string()),
                skip_git_check: true,
                json_output: false,
            }),
            None,
            None,
            None,
        );

        let sdk = match execution {
            ExecutionMode::Sdk(sdk) => sdk,
            _ => panic!("codex should use SDK execution"),
        };

        assert_eq!(sdk.program, "codex");
        assert!(sdk.args.iter().any(|arg| arg == "exec"));
        assert!(sdk.args.iter().any(|arg| arg == "--json"));
        assert!(sdk.args.iter().any(|arg| arg == "--skip-git-repo-check"));
        assert_eq!(sdk.args.last().map(String::as_str), Some("-"));
        assert_eq!(
            sdk.stdin_injection.as_deref(),
            Some("Implement the feature")
        );
    }

    #[test]
    fn codex_exec_omits_full_auto_for_legacy_approval_modes() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Review the branch",
            &AgentTypeConfig::Codex(CodexConfig {
                model: None,
                sandbox: None,
                approval_mode: Some("suggest".to_string()),
                skip_git_check: false,
                json_output: false,
            }),
            None,
            None,
            None,
        );

        let sdk = match execution {
            ExecutionMode::Sdk(sdk) => sdk,
            _ => panic!("codex should use SDK execution"),
        };

        assert!(!sdk.args.iter().any(|arg| arg == "--full-auto"));
    }

    #[test]
    fn codex_exec_clamps_reasoning_for_fast_model() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Implement the feature",
            &AgentTypeConfig::Codex(CodexConfig {
                model: Some("gpt-5-codex-mini".to_string()),
                sandbox: None,
                approval_mode: None,
                skip_git_check: false,
                json_output: false,
            }),
            None,
            None,
            None,
        );

        let sdk = match execution {
            ExecutionMode::Sdk(sdk) => sdk,
            _ => panic!("codex should use SDK execution"),
        };

        assert!(sdk.args.iter().any(|arg| arg == "-c"));
        assert!(sdk
            .args
            .iter()
            .any(|arg| arg == "model_reasoning_effort=\"medium\""));
    }

    #[test]
    fn codex_interactive_clamps_reasoning_for_fast_model() {
        let shell = build_interactive_terminal_command(
            &AgentType::Codex,
            &AgentTypeConfig::Codex(CodexConfig {
                model: Some("gpt-5-codex-mini".to_string()),
                sandbox: Some("workspace-write".to_string()),
                approval_mode: Some("full-auto".to_string()),
                skip_git_check: true,
                json_output: false,
            }),
            None,
        );

        assert_eq!(shell.program, "codex");
        assert!(shell.args.iter().any(|arg| arg == "--full-auto"));
        assert!(shell.args.iter().any(|arg| arg == "--skip-git-repo-check"));
        assert!(shell.args.iter().any(|arg| arg == "-c"));
        assert!(shell
            .args
            .iter()
            .any(|arg| arg == "model_reasoning_effort=\"medium\""));
    }
}
