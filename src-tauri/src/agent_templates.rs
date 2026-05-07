use crate::models::{
    AgentType, AgentTypeConfig, ClaudeCodeConfig, CodexConfig, ExecutionMode, GeminiConfig,
    SdkExecution, ShellExecution,
};

const CODEX_IGNORE_USER_CONFIG_ARG: &str = "--ignore-user-config";

/// Builds the execution mode for a given agent type and prompt.
///
/// Claude Code, Codex, and Gemini use structured SDK mode.
/// Custom agents stay on the PTY path.
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

    // Compare nodes pick one winner. Synthesis nodes combine useful work across
    // branches into one coherent result.
    if node_type == Some("merge") {
        effective_prompt.push_str(
            "\n\nIMPORTANT — COMPARE PROCEDURE (follow these steps exactly):\n\
             1. Review the sibling_diffs above to compare each approach.\n\
             2. Decide which single approach is best. Do not combine approaches in this step.\n\
             3. Run `git merge <winning-branch-name> --no-edit` to bring the winning code into your worktree. \
                The branch names are listed in sibling_diffs above.\n\
             4. If there are merge conflicts, resolve them.\n\
             5. Write CRONGEN_DECISION.md at the repo root with the complete decision tree, conclusions, and these sections: \
                # Crongen Decision Report, Flow Summary, Branch Outcomes, Decision, Integrated Result, \
                Rejected or Deferred Ideas, Validation, Next Steps. Explain why the winning branch was chosen.\n\
             6. Commit all changes including CRONGEN_DECISION.md.",
        );
    } else if node_type == Some("synthesis") {
        effective_prompt.push_str(
            "\n\nIMPORTANT — SYNTHESIS PROCEDURE (follow these steps exactly):\n\
             1. Review the sibling_diffs above to understand each branch's implementation and tradeoffs.\n\
             2. Identify which ideas, files, or code paths are worth combining into a better solution.\n\
             3. Merge, cherry-pick, or manually port the useful parts from sibling branches into one coherent implementation. \
                Remove duplicated, conflicting, or weaker code paths.\n\
             4. Resolve conflicts and verify the final implementation is internally consistent.\n\
             5. Write CRONGEN_DECISION.md at the repo root with the complete decision tree, conclusions, and these sections: \
                # Crongen Decision Report, Flow Summary, Branch Outcomes, Decision, Integrated Result, \
                Rejected or Deferred Ideas, Validation, Next Steps. Explain what was taken from each branch \
                and why the synthesized result is better than any single branch.\n\
             6. Commit all changes including CRONGEN_DECISION.md.",
        );
    }

    if !matches!(node_type, Some("validation")) {
        effective_prompt.push_str(
            "\n\nBefore finishing: if you changed files, create a git commit with a concise message that explains the actual change. Do not use generic messages like \"auto commit\" or \"agent work\".",
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
            build_gemini_headless_command(&effective_prompt, cfg, default_model)
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

    append_extra_args(&mut args, &cfg.extra_args);

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

    append_extra_args(&mut args, &cfg.extra_args);

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
    args.push(CODEX_IGNORE_USER_CONFIG_ARG.to_string());
    args.push("--json".to_string());

    // Optional flags
    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());

        if matches!(model, "gpt-5-codex-mini" | "codex-1p-mini-q-20251105-ev3") {
            args.push("-c".to_string());
            args.push("model_reasoning_effort=\"medium\"".to_string());
        }
    }

    if let Some(sandbox) = codex_sandbox_mode(cfg) {
        args.push("--sandbox".to_string());
        args.push(sandbox.to_string());
    }

    if cfg.skip_git_check {
        args.push("--skip-git-repo-check".to_string());
    }

    if cfg.json_output {
        // Already enforced above; keep config field harmlessly compatible.
    }

    append_extra_args(&mut args, &cfg.extra_args);

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

    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());

        if matches!(model, "gpt-5-codex-mini" | "codex-1p-mini-q-20251105-ev3") {
            args.push("-c".to_string());
            args.push("model_reasoning_effort=\"medium\"".to_string());
        }
    }

    if let Some(sandbox) = codex_sandbox_mode(cfg) {
        args.push("--sandbox".to_string());
        args.push(sandbox.to_string());
    }

    if cfg.skip_git_check {
        args.push("--skip-git-repo-check".to_string());
    }

    append_extra_args(&mut args, &cfg.extra_args);

    ShellExecution {
        program: "codex".to_string(),
        args,
        stdin_injection: None,
        auto_responses: vec![],
    }
}

fn codex_sandbox_mode(cfg: &CodexConfig) -> Option<&str> {
    if let Some(sandbox) = cfg.sandbox.as_deref() {
        return Some(sandbox);
    }

    match cfg.approval_mode.as_deref().unwrap_or("full-auto") {
        "full-auto" => Some("workspace-write"),
        // Legacy values don't map 1:1 to the current Codex exec CLI.
        // Falling back to the CLI default keeps runtime and previews aligned.
        "suggest" | "auto-edit" | "default" | "" => None,
        _ => None,
    }
}

fn build_gemini_headless_command(
    prompt: &str,
    cfg: &GeminiConfig,
    default_model: Option<&str>,
) -> ExecutionMode {
    let mut args = Vec::new();

    if cfg.yolo {
        args.push("--yolo".to_string());
    }

    if let Some(model) = cfg.model.as_deref().or(default_model) {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    if cfg
        .sandbox
        .as_deref()
        .is_some_and(|value| value != "false" && value != "0")
    {
        args.push("--sandbox".to_string());
    }

    args.push("--output-format".to_string());
    args.push("stream-json".to_string());
    append_extra_args(&mut args, &cfg.extra_args);
    args.push("--prompt".to_string());
    args.push(String::new());

    ExecutionMode::Sdk(SdkExecution {
        program: "gemini".to_string(),
        args,
        stdin_injection: Some(prompt.to_string()),
    })
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

    if cfg
        .sandbox
        .as_deref()
        .is_some_and(|value| value != "false" && value != "0")
    {
        args.push("--sandbox".to_string());
    }

    append_extra_args(&mut args, &cfg.extra_args);

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

fn append_extra_args(args: &mut Vec<String>, extra_args: &[String]) {
    args.extend(
        extra_args
            .iter()
            .map(|arg| arg.trim())
            .filter(|arg| !arg.is_empty())
            .map(ToString::to_string),
    );
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
                extra_args: vec![],
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
        assert!(sdk.args.iter().any(|arg| arg == "--ignore-user-config"));
        assert!(sdk.args.iter().any(|arg| arg == "--json"));
        assert!(sdk.args.iter().any(|arg| arg == "--skip-git-repo-check"));
        assert_eq!(sdk.args.last().map(String::as_str), Some("-"));
        let stdin = sdk.stdin_injection.as_deref().expect("stdin prompt");
        assert!(stdin.starts_with("Implement the feature"));
        assert!(stdin.contains("create a git commit with a concise message"));
    }

    #[test]
    fn synthesis_prompt_requires_decision_report_and_combined_result() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Resolve the alternatives",
            &AgentTypeConfig::Codex(CodexConfig {
                model: Some("gpt-5.4".to_string()),
                extra_args: vec![],
                sandbox: Some("workspace-write".to_string()),
                approval_mode: Some("full-auto".to_string()),
                skip_git_check: true,
                json_output: false,
            }),
            None,
            Some("synthesis"),
            None,
        );

        let sdk = match execution {
            ExecutionMode::Sdk(sdk) => sdk,
            _ => panic!("codex should use SDK execution"),
        };
        let prompt = sdk.stdin_injection.as_deref().expect("stdin prompt");

        assert!(prompt.contains("SYNTHESIS PROCEDURE"));
        assert!(prompt.contains("CRONGEN_DECISION.md"));
        assert!(prompt.contains("better than any single branch"));
    }

    #[test]
    fn codex_exec_uses_workspace_write_for_full_auto_mode() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Review the branch",
            &AgentTypeConfig::Codex(CodexConfig {
                model: None,
                extra_args: vec![],
                sandbox: None,
                approval_mode: Some("full-auto".to_string()),
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

        assert!(sdk.args.iter().any(|arg| arg == "--ignore-user-config"));
        assert!(!sdk.args.iter().any(|arg| arg == "--full-auto"));
        assert!(sdk.args.iter().any(|arg| arg == "--sandbox"));
        assert!(sdk.args.iter().any(|arg| arg == "workspace-write"));
    }

    #[test]
    fn codex_exec_omits_sandbox_for_legacy_approval_modes() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Review the branch",
            &AgentTypeConfig::Codex(CodexConfig {
                model: None,
                extra_args: vec![],
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

        assert!(sdk.args.iter().any(|arg| arg == "--ignore-user-config"));
        assert!(!sdk.args.iter().any(|arg| arg == "--full-auto"));
        assert!(!sdk.args.iter().any(|arg| arg == "--sandbox"));
    }

    #[test]
    fn codex_exec_clamps_reasoning_for_fast_model() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Implement the feature",
            &AgentTypeConfig::Codex(CodexConfig {
                model: Some("gpt-5-codex-mini".to_string()),
                extra_args: vec![],
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
        assert!(
            sdk.args
                .iter()
                .any(|arg| arg == "model_reasoning_effort=\"medium\"")
        );
    }

    #[test]
    fn codex_exec_appends_extra_args_before_prompt_stdin_marker() {
        let execution = build_shell_command(
            &AgentType::Codex,
            "Implement the feature",
            &AgentTypeConfig::Codex(CodexConfig {
                model: None,
                sandbox: None,
                approval_mode: None,
                skip_git_check: false,
                json_output: false,
                extra_args: vec!["--search".to_string()],
            }),
            None,
            None,
            None,
        );

        let sdk = match execution {
            ExecutionMode::Sdk(sdk) => sdk,
            _ => panic!("codex should use SDK execution"),
        };

        assert!(
            sdk.args
                .ends_with(&["--search".to_string(), "-".to_string()])
        );
    }

    #[test]
    fn codex_interactive_clamps_reasoning_for_fast_model() {
        let shell = build_interactive_terminal_command(
            &AgentType::Codex,
            &AgentTypeConfig::Codex(CodexConfig {
                model: Some("gpt-5-codex-mini".to_string()),
                extra_args: vec![],
                sandbox: Some("workspace-write".to_string()),
                approval_mode: Some("full-auto".to_string()),
                skip_git_check: true,
                json_output: false,
            }),
            None,
        );

        assert_eq!(shell.program, "codex");
        assert!(!shell.args.iter().any(|arg| arg == "--full-auto"));
        assert!(shell.args.iter().any(|arg| arg == "--sandbox"));
        assert!(shell.args.iter().any(|arg| arg == "--skip-git-repo-check"));
        assert!(shell.args.iter().any(|arg| arg == "-c"));
        assert!(
            shell
                .args
                .iter()
                .any(|arg| arg == "model_reasoning_effort=\"medium\"")
        );
    }

    #[test]
    fn gemini_headless_uses_prompt_and_stream_json() {
        let execution = build_shell_command(
            &AgentType::Gemini,
            "Implement the feature",
            &AgentTypeConfig::Gemini(GeminiConfig {
                model: Some("gemini-3-pro".to_string()),
                extra_args: vec!["--include-directories".to_string(), "../shared".to_string()],
                sandbox: Some("true".to_string()),
                yolo: true,
            }),
            None,
            None,
            None,
        );

        let sdk = match execution {
            ExecutionMode::Sdk(sdk) => sdk,
            _ => panic!("gemini should use SDK execution"),
        };

        assert_eq!(sdk.program, "gemini");
        assert!(sdk.args.iter().any(|arg| arg == "--prompt"));
        let stdin = sdk.stdin_injection.as_deref().expect("stdin prompt");
        assert!(stdin.starts_with("Implement the feature"));
        assert!(stdin.contains("create a git commit with a concise message"));
        assert!(sdk.args.iter().any(|arg| arg == "--output-format"));
        assert!(sdk.args.iter().any(|arg| arg == "stream-json"));
        assert!(sdk.args.iter().any(|arg| arg == "--yolo"));
        assert!(sdk.args.iter().any(|arg| arg == "--sandbox"));
        assert!(
            sdk.args
                .windows(2)
                .any(|pair| pair == ["--include-directories", "../shared"])
        );
    }
}
