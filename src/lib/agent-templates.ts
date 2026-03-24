import type {
  AgentType,
  AgentTypeConfig,
  ClaudeCodeConfig,
  CodexConfig,
  GeminiConfig,
  CustomConfig,
} from "../types";

// ─── Template Metadata ─────────────────────────────────────────

export interface AgentTypeTemplate {
  type: AgentType;
  label: string;
  description: string;
  executable: string;
  envVar: string | null;
  defaultConfig: AgentTypeConfig;
  buildCommandPreview: (prompt: string, config: AgentTypeConfig) => string;
}

function codexApprovalPreviewFlag(mode?: string | null): string | null {
  return !mode || mode === "full-auto" ? "--full-auto" : null;
}

export const CLAUDE_CODE_TEMPLATE: AgentTypeTemplate = {
  type: "claude_code",
  label: "Claude Code",
  description: "Anthropic's coding agent CLI",
  executable: "claude",
  envVar: null,
  defaultConfig: {
    type: "claude_code",
    model: null,
    max_turns: null,
    max_budget_usd: null,
    allowed_tools: null,
    disallowed_tools: null,
    append_system_prompt: null,
    dangerously_skip_permissions: true,
  } satisfies ClaudeCodeConfig,
  buildCommandPreview: (prompt, config) => {
    const cfg = config as ClaudeCodeConfig;
    const parts = ["claude", "-p", JSON.stringify(prompt), "--output-format", "stream-json", "--verbose"];
    if (cfg.dangerously_skip_permissions)
      parts.push("--dangerously-skip-permissions");
    if (cfg.model) parts.push("--model", cfg.model);
    if (cfg.max_turns) parts.push("--max-turns", String(cfg.max_turns));
    if (cfg.max_budget_usd)
      parts.push("--max-cost", cfg.max_budget_usd.toFixed(2));
    if (cfg.allowed_tools)
      cfg.allowed_tools
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean)
        .forEach((t) => parts.push("--allowedTools", t));
    if (cfg.disallowed_tools)
      cfg.disallowed_tools
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean)
        .forEach((t) => parts.push("--disallowedTools", t));
    if (cfg.append_system_prompt)
      parts.push("--append-system-prompt", JSON.stringify(cfg.append_system_prompt));
    return parts.join(" ");
  },
};

export const CODEX_TEMPLATE: AgentTypeTemplate = {
  type: "codex",
  label: "Codex",
  description: "OpenAI's coding agent CLI",
  executable: "codex",
  envVar: null,
  defaultConfig: {
    type: "codex",
    model: null,
    sandbox: null,
    approval_mode: null,
    skip_git_check: false,
    json_output: false,
  } satisfies CodexConfig,
  buildCommandPreview: (prompt, config) => {
    const cfg = config as CodexConfig;
    const parts = ["codex", "exec", "--json"];
    const approvalFlag = codexApprovalPreviewFlag(cfg.approval_mode);
    if (approvalFlag) parts.push(approvalFlag);
    if (cfg.model) parts.push("--model", cfg.model);
    if (cfg.sandbox) parts.push("--sandbox", cfg.sandbox);
    if (cfg.skip_git_check) parts.push("--skip-git-repo-check");
    parts.push("-");
    return `printf %s ${JSON.stringify(prompt)} | ${parts.join(" ")}`;
  },
};

export const GEMINI_TEMPLATE: AgentTypeTemplate = {
  type: "gemini",
  label: "Gemini CLI",
  description: "Google's Gemini coding agent CLI",
  executable: "gemini",
  envVar: null,
  defaultConfig: {
    type: "gemini",
    model: null,
    sandbox: null,
    yolo: true,
  } satisfies GeminiConfig,
  buildCommandPreview: (prompt, config) => {
    const cfg = config as GeminiConfig;
    const parts = ["gemini"];
    if (cfg.yolo) parts.push("--yolo");
    if (cfg.model) parts.push("--model", cfg.model);
    if (cfg.sandbox) parts.push("--sandbox", cfg.sandbox);
    return `echo ${JSON.stringify(prompt)} | ${parts.join(" ")}`;
  },
};

export const CUSTOM_TEMPLATE: AgentTypeTemplate = {
  type: "custom",
  label: "Custom",
  description: "Any CLI tool or shell script",
  executable: "bash",
  envVar: null,
  defaultConfig: {
    type: "custom",
    shell: "bash",
  } satisfies CustomConfig,
  buildCommandPreview: (prompt, config) => {
    const cfg = config as CustomConfig;
    const shell = cfg.shell || "bash";
    if (prompt) {
      return `${shell} <<< ${JSON.stringify(prompt)}`;
    }
    return shell;
  },
};

export const AGENT_TEMPLATES: Record<AgentType, AgentTypeTemplate> = {
  claude_code: CLAUDE_CODE_TEMPLATE,
  codex: CODEX_TEMPLATE,
  gemini: GEMINI_TEMPLATE,
  custom: CUSTOM_TEMPLATE,
};

export const BUILT_IN_AGENT_TYPES: AgentType[] = ["claude_code", "codex", "gemini"];

export function getAgentLabel(agentType: AgentType | null | undefined): string {
  if (!agentType) return "Unconfigured";
  return AGENT_TEMPLATES[agentType]?.label ?? "Unknown";
}
