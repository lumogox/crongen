// ─── Project / Runtime Agent Types ─────────────────────────────

export type AgentType = "claude_code" | "codex" | "gemini" | "custom";
export type AgentRole = "planning" | "execution";
export type AgentProviderStatus = "ready" | "missing_cli" | "needs_login" | "coming_soon" | "error";

export interface ClaudeCodeConfig {
  type: "claude_code";
  model?: string | null;
  max_turns?: number | null;
  max_budget_usd?: number | null;
  allowed_tools?: string | null;
  disallowed_tools?: string | null;
  append_system_prompt?: string | null;
  dangerously_skip_permissions: boolean;
}

export interface CodexConfig {
  type: "codex";
  model?: string | null;
  sandbox?: string | null;
  approval_mode?: string | null;
  skip_git_check: boolean;
  json_output: boolean;
}

export interface GeminiConfig {
  type: "gemini";
  model?: string | null;
  sandbox?: string | null;
  yolo: boolean;
}

export interface CustomConfig {
  type: "custom";
  shell?: string | null;
}

export type AgentTypeConfig = ClaudeCodeConfig | CodexConfig | GeminiConfig | CustomConfig;

export interface Project {
  id: string;
  name: string;
  prompt: string;         // Used as "description" in the UI
  shell: string;
  repo_path: string;
  is_active: boolean;
  agent_type: AgentType;
  type_config: AgentTypeConfig;
  project_mode: "blank" | "existing";
  created_at: number;
  updated_at: number;
}

export interface AgentProviderReadiness {
  agent_type: AgentType;
  status: AgentProviderStatus;
  detail?: string | null;
  ready: boolean;
  supports_planning: boolean;
  supports_execution: boolean;
  coming_soon: boolean;
}

// ─── Decision Node ─────────────────────────────────────────────

export type NodeStatus =
  | "pending"
  | "running"
  | "paused"
  | "completed"
  | "failed"
  | "merged";

export interface DecisionNode {
  id: string;
  project_id: string;
  parent_id: string | null;
  label: string;
  prompt: string;
  branch_name: string;
  worktree_path: string | null;
  commit_hash: string | null;
  status: NodeStatus;
  exit_code: number | null;
  node_type: string | null;
  scheduled_at: string | null; // ISO 8601 — only meaningful on root (session) nodes
  created_at: number;
  updated_at: number;
}

export interface NodeRuntimeValidation {
  node: DecisionNode;
  session_active: boolean;
  session_backend?: string | null;
  reconciled: boolean;
  message: string;
}

// ─── Git Operations ───────────────────────────────────────────

export interface MergeResult {
  success: boolean;
  merge_commit_hash: string | null;
  conflict_files: string[];
  auto_resolved: boolean;
  resolution_summary: string | null;
}

// ─── React Flow Node Data ──────────────────────────────────────

import type { VisualNodeType } from "./node-types";

export interface DecisionNodeData {
  [key: string]: unknown;
  node: DecisionNode;
  isSelected: boolean;
  visualType: VisualNodeType;
  onFork: (nodeId: string) => void;
  onMerge: (nodeId: string) => void;
  flowMode: "linear" | "branching";
  onRunNode: (nodeId: string) => void;
  onUpdateNode: (nodeId: string, label: string, prompt: string) => void;
  onCreateStructuralNode: (parentId: string | null, nodeType: "task" | "decision" | "agent" | "merge" | "final") => void;
  onDeleteNode: (nodeId: string) => void;
  isOrchestratorTarget?: boolean;
  debugMode?: boolean;
  onResetNode?: (nodeId: string) => void;
}

// ─── Orchestrator ──────────────────────────────────────────────

export type OrchestratorMode = "auto" | "supervised";
export type OrchestratorState = "idle" | "running" | "waiting_user" | "complete" | "failed";

export interface OrchestratorStatus {
  session_id: string;
  state: OrchestratorState;
  mode: OrchestratorMode;
  current_node_id: string | null;
  completed_count: number;
  total_count: number;
  pending_decision: PendingDecision | null;
}

export interface PendingDecision {
  node_id: string;
  label: string;
  prompt: string;
  options: DecisionOption[];
}

export interface DecisionOption {
  node_id: string;
  label: string;
  prompt: string;
}

// ─── Settings ──────────────────────────────────────────────────

export interface AppSettings {
  debug_mode: boolean;
  agent_setup_seen?: boolean;
  planning_agent?: AgentType | null;
  execution_agent?: AgentType | null;
  planning_model?: string | null;
  execution_model?: string | null;
}

// ─── App State ─────────────────────────────────────────────────

export type ModalType =
  | { kind: "create_project" }
  | { kind: "edit_project"; project: Project }
  | { kind: "fork_node"; nodeId: string }
  | { kind: "create_structural_node"; parentId: string | null; nodeType: "task" | "decision" | "agent" | "merge" | "final" }
  | { kind: "delete_project"; project: Project }
  | { kind: "delete_node"; node: DecisionNode }
  | { kind: "create_session" }
  | { kind: "orchestrator_decision"; sessionId: string; decision: PendingDecision }
  | { kind: "settings"; forceSetup?: boolean; onboarding?: boolean; focusRole?: AgentRole }
  | null;
