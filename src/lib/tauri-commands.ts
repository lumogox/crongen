import { invoke } from "@tauri-apps/api/core";
import type { Agent, AgentTypeConfig, AppSettings, DecisionNode, MergeResult, OrchestratorStatus } from "../types";

// ─── Agent CRUD ────────────────────────────────────────────────

export async function createAgent(params: {
  name: string;
  prompt: string;
  repoPath: string;
  agentType: string;
  typeConfig: AgentTypeConfig;
  projectMode?: string;
}): Promise<Agent> {
  return invoke("create_agent", {
    name: params.name,
    prompt: params.prompt,
    repoPath: params.repoPath,
    agentType: params.agentType,
    typeConfig: params.typeConfig,
    projectMode: params.projectMode,
  });
}

export async function getAgents(): Promise<Agent[]> {
  return invoke("get_agents");
}

export async function getAgent(id: string): Promise<Agent> {
  return invoke("get_agent", { id });
}

export async function updateAgent(params: {
  id: string;
  name: string;
  prompt: string;
  repoPath: string;
  agentType: string;
  typeConfig: AgentTypeConfig;
  isActive: boolean;
  projectMode?: string;
}): Promise<Agent> {
  return invoke("update_agent", {
    id: params.id,
    name: params.name,
    prompt: params.prompt,
    repoPath: params.repoPath,
    agentType: params.agentType,
    typeConfig: params.typeConfig,
    isActive: params.isActive,
    projectMode: params.projectMode,
  });
}

export async function deleteAgent(id: string): Promise<void> {
  return invoke("delete_agent", { id });
}

export async function toggleAgent(
  id: string,
  isActive: boolean,
): Promise<Agent> {
  return invoke("toggle_agent", { id, isActive });
}

// ─── Decision Tree ─────────────────────────────────────────────

export async function getDecisionTree(
  agentId: string,
): Promise<DecisionNode[]> {
  return invoke("get_decision_tree", { agentId });
}

// ─── Git + Node Operations ────────────────────────────────────

export async function runAgentNow(id: string): Promise<DecisionNode> {
  return invoke("run_agent_now", { id });
}

export async function forkNode(
  nodeId: string,
  label: string,
  prompt: string,
): Promise<DecisionNode> {
  return invoke("fork_node", { nodeId, label, prompt });
}

export async function mergeNodeBranch(
  nodeId: string,
): Promise<MergeResult> {
  return invoke("merge_node_branch", { nodeId });
}

export async function createStructuralNode(params: {
  agentId: string;
  parentId: string | null;
  label: string;
  prompt: string;
  nodeType: string;
}): Promise<DecisionNode> {
  return invoke("create_structural_node", {
    agentId: params.agentId,
    parentId: params.parentId,
    label: params.label,
    prompt: params.prompt,
    nodeType: params.nodeType,
  });
}

export async function createRootNode(
  agentId: string,
  label: string,
  prompt: string,
): Promise<DecisionNode> {
  return invoke("create_root_node", { agentId, label, prompt });
}

export async function runNode(nodeId: string): Promise<DecisionNode> {
  return invoke("run_node", { nodeId });
}

export async function updateNode(
  nodeId: string,
  label: string,
  prompt: string,
): Promise<DecisionNode> {
  return invoke("update_node", { nodeId, label, prompt });
}

export async function getRootNodes(agentId: string): Promise<DecisionNode[]> {
  return invoke("get_root_nodes", { agentId });
}

export async function deleteNodeBranch(
  nodeId: string,
): Promise<string[]> {
  return invoke("delete_node_branch", { nodeId });
}

// ─── PTY Commands ─────────────────────────────────────────────

export async function writePty(
  sessionId: string,
  data: string,
): Promise<void> {
  return invoke("write_pty", { sessionId, data });
}

export async function resizePty(
  sessionId: string,
  rows: number,
  cols: number,
): Promise<void> {
  return invoke("resize_pty", { sessionId, rows, cols });
}

export async function getSessionOutput(
  sessionId: string,
): Promise<string | null> {
  return invoke("get_session_output", { sessionId });
}

export async function pauseSession(sessionId: string): Promise<void> {
  return invoke("pause_session", { sessionId });
}

export async function resumeSession(sessionId: string): Promise<void> {
  return invoke("resume_session", { sessionId });
}

// ─── SDK Commands ─────────────────────────────────────────────

export async function getSdkSessionOutput(
  sessionId: string,
): Promise<string[]> {
  return invoke("get_sdk_session_output", { sessionId });
}

// ─── Utility ───────────────────────────────────────────────────

export async function checkExecutable(name: string): Promise<boolean> {
  return invoke("check_executable", { name });
}

export async function checkEnvVar(name: string): Promise<boolean> {
  return invoke("check_env_var", { name });
}

// ─── Orchestrator ─────────────────────────────────────────────

export async function startOrchestrator(
  sessionRootId: string,
  mode: string,
): Promise<void> {
  return invoke("start_orchestrator", { sessionRootId, mode });
}

export async function getOrchestratorStatus(
  sessionRootId: string,
): Promise<OrchestratorStatus | null> {
  return invoke("get_orchestrator_status", { sessionRootId });
}

export async function submitOrchestratorDecision(
  sessionRootId: string,
  selectedNodeId: string,
): Promise<void> {
  return invoke("submit_orchestrator_decision", { sessionRootId, selectedNodeId });
}

export async function cancelOrchestrator(
  sessionRootId: string,
): Promise<void> {
  return invoke("cancel_orchestrator", { sessionRootId });
}

// ─── Settings ──────────────────────────────────────────────────

export async function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export async function updateSettings(settings: AppSettings): Promise<void> {
  return invoke("update_settings", { settings });
}

export async function resetNodeStatus(nodeId: string): Promise<DecisionNode> {
  return invoke("reset_node_status", { nodeId });
}

// ─── Merge Preview ────────────────────────────────────────────

export interface MergePreview {
  source_branch: string;
  target_branch: string;
  commit_count: number;
  files_changed: string[];
}

export async function getMergePreview(nodeId: string): Promise<MergePreview> {
  return invoke("get_merge_preview", { nodeId });
}

// ─── Git Branch Operations ────────────────────────────────────

export async function getRepoBranch(agentId: string): Promise<string> {
  return invoke("get_repo_branch", { agentId });
}

export async function createFeatureBranch(
  nodeId: string,
  branchName: string,
): Promise<string> {
  return invoke("create_feature_branch", { nodeId, branchName });
}

export async function markNodeMerged(nodeId: string): Promise<void> {
  return invoke("mark_node_merged", { nodeId });
}

// ─── Debug ────────────────────────────────────────────────────

export async function getNodeContext(nodeId: string): Promise<string> {
  return invoke("get_node_context", { nodeId });
}

// ─── Plan Generation ──────────────────────────────────────────

export async function generatePlan(
  agentId: string,
  prompt: string,
  complexity?: "linear" | "branching",
): Promise<DecisionNode[]> {
  return invoke("generate_plan", { agentId, prompt, complexity });
}
