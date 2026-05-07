import { useState } from "react";
import {
  GitFork,
  GitMerge,
  Combine,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  CornerDownLeft,
  Square,
  Trash2,
  ScrollText,
  Trophy,
  Plus,
  SquareTerminal,
  CheckCircle2,
} from "lucide-react";
import type { AgentType, DecisionNode, Project } from "../types";
import type { StructuralNodeType, VisualNodeType } from "../types/node-types";
import { getAgentLabel } from "../lib/agent-templates";
import { usesPtySessionControls } from "../lib/agent-runtime";
import { getNodeTypeMeta, inferNodeType } from "../lib/node-type-inference";
import { formatRelativeTime, formatSessionRuntime } from "../lib/utils";
import { Button } from "./ui/button";
import { SessionTab } from "./inspector/SessionTab";

type InspectorTab = "Overview" | "Session" | "Actions";

interface InspectorPanelProps {
  node: DecisionNode;
  project: Project;
  allNodes: DecisionNode[];
  flowMode: "linear" | "branching";
  onClose: () => void;
  onFork: (nodeId: string) => void;
  onMerge: (nodeId: string) => void;
  onCreateStructuralNode?: (parentId: string | null, nodeType: StructuralNodeType) => void;
  onPause?: (nodeId: string) => void;
  onResume?: (nodeId: string) => void;
  onDelete?: (nodeId: string) => void;
  onRunNode?: (nodeId: string) => void;
  onUpdateNode?: (nodeId: string, label: string, prompt: string) => void;
  onUpdateNodeAgent?: (nodeId: string, agentType: AgentType | null) => void;
  onValidateRuntime?: (nodeId: string) => void;
  onSendEnter?: (nodeId: string) => void;
  onStop?: (nodeId: string) => void;
  onRetryNode?: (nodeId: string) => void;
  onResetNode?: (nodeId: string) => void;
  onOpenTerminal?: (nodeId: string) => void;
  defaultExecutionAgent?: AgentType | null;
  manualTerminalSessionId?: string | null;
}

const statusTones: Record<string, string> = {
  pending: "bg-slate-500/10 text-slate-300 border-slate-400/20",
  running: "bg-amber-500/10 text-amber-200 border-amber-400/20",
  paused: "bg-sky-500/10 text-sky-200 border-sky-400/20",
  completed: "bg-emerald-500/10 text-emerald-200 border-emerald-400/20",
  failed: "bg-rose-500/10 text-rose-200 border-rose-400/20",
  merged: "bg-violet-500/10 text-violet-200 border-violet-400/20",
};

const statusLabels: Record<string, string> = {
  pending: "Queued",
  running: "Running",
  paused: "Paused",
  completed: "Succeeded",
  failed: "Failed",
  merged: "Merged",
};

export function InspectorPanel({
  node,
  project,
  allNodes,
  flowMode,
  onClose: _onClose,
  onFork: _onFork,
  onMerge,
  onCreateStructuralNode,
  onPause,
  onResume,
  onDelete,
  onRunNode,
  onUpdateNode,
  onUpdateNodeAgent,
  onValidateRuntime,
  onSendEnter,
  onStop,
  onRetryNode,
  onResetNode,
  onOpenTerminal,
  defaultExecutionAgent,
  manualTerminalSessionId,
}: InspectorPanelProps) {
  const [activeTab, setActiveTab] = useState<InspectorTab>("Overview");
  const visualType: VisualNodeType = inferNodeType(node, allNodes);
  const typeMeta = getNodeTypeMeta(visualType);

  const isTerminal =
    node.status === "completed" ||
    node.status === "failed" ||
    node.status === "paused";
  const canPause = node.status === "running";
  const canResume = node.status === "paused";
  const canDelete = node.status !== "running";
  const isStructural = ["decision", "merge", "synthesis", "final"].includes(visualType);

  const tabs: InspectorTab[] = ["Overview", "Session", "Actions"];

  const children = allNodes.filter((n) => n.parent_id === node.id);
  const isResolutionNode = visualType === "merge" || visualType === "synthesis" || node.status === "merged";
  const isSessionRoot = node.parent_id === null;
  const effectiveAgent = node.agent_type_override ?? defaultExecutionAgent ?? project.agent_type;
  const canAssignAgent = ["task", "agent", "merge", "synthesis", "final"].includes(visualType);
  const assignableAgents: AgentType[] = ["claude_code", "codex", "gemini"];

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-[1.75rem] border border-slate-700/70 bg-[#121a2a] shadow-xl">
      {/* Header */}
      <div className="border-b border-slate-700/70 px-6 pt-5 pb-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[11px] uppercase tracking-[0.22em] text-slate-400">
              Inspector
            </div>
            <div className="mt-2 text-xl font-semibold text-slate-50 truncate">
              {node.label}
            </div>
            <div className="mt-1 text-sm text-slate-400 truncate">
              {node.prompt}
            </div>
          </div>
          <span
            className={`inline-flex shrink-0 items-center rounded-full border px-2 py-1 text-[11px] font-medium ${statusTones[node.status] ?? statusTones.pending}`}
          >
            {isStructural && node.status === "pending"
              ? "Needs review"
              : (statusLabels[node.status] ?? node.status)}
          </span>
        </div>

        {/* Type / Owner grid */}
        <div className="mt-4 grid grid-cols-2 gap-3 text-sm">
          <div className="rounded-2xl border border-slate-700/70 bg-[#182235] p-3">
            <div className="text-xs text-slate-400">Type</div>
            <div className="mt-1 text-slate-100">{typeMeta.label}</div>
          </div>
          <div className="rounded-2xl border border-slate-700/70 bg-[#182235] p-3">
            <div className="text-xs text-slate-400">Project</div>
            <div className="mt-1 text-slate-100">{project.name}</div>
          </div>
          {canAssignAgent && (
            <div className="col-span-2 rounded-2xl border border-slate-700/70 bg-[#182235] p-3">
              <div className="mb-2 flex items-center justify-between gap-3">
                <div>
                  <div className="text-xs text-slate-400">Agent</div>
                  <div className="mt-1 text-slate-100">
                    {node.agent_type_override
                      ? getAgentLabel(node.agent_type_override)
                      : `Default (${getAgentLabel(effectiveAgent)})`}
                  </div>
                </div>
                <select
                  value={node.agent_type_override ?? ""}
                  disabled={!onUpdateNodeAgent || node.status === "running"}
                  onChange={(event) => {
                    const nextAgent = event.target.value
                      ? (event.target.value as AgentType)
                      : null;
                    onUpdateNodeAgent?.(node.id, nextAgent);
                  }}
                  className="min-w-40 rounded-xl border border-slate-600/70 bg-[#0f1726] px-3 py-2 text-sm text-slate-100 outline-none transition-colors hover:border-slate-500 focus:border-sky-400/70 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <option value="">Use default</option>
                  {assignableAgents.map((agent) => (
                    <option key={agent} value={agent}>
                      {getAgentLabel(agent)}
                    </option>
                  ))}
                </select>
              </div>
              <div className="text-xs leading-5 text-slate-400">
                Used when this node runs. Leave default to follow Agent Bay settings.
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Tab bar — pill style */}
      <div className="border-b border-slate-700/70 px-6 py-3">
        <div className="flex gap-2">
          {tabs.map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`rounded-full px-3 py-1.5 text-sm transition-colors ${
                activeTab === tab
                  ? "bg-slate-100 text-slate-950"
                  : "bg-[#182235] text-slate-300 hover:bg-[#243044]"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden relative">
        {/* Session tab stays mounted for terminal preservation */}
        <SessionTab
          node={node}
          agentType={effectiveAgent}
          isActive={activeTab === "Session"}
          manualTerminalSessionId={manualTerminalSessionId}
        />

        {activeTab === "Overview" && (
          <div className="absolute inset-0 overflow-y-auto p-6 space-y-4">
            {/* Editable fields for pending nodes */}
            {node.status === "pending" && !node.worktree_path && onUpdateNode && (
              <div className="space-y-3">
                <div className="rounded-2xl border border-sky-400/20 bg-sky-500/10 p-3">
                  <div className="text-[11px] uppercase tracking-[0.18em] text-sky-300 mb-2">Designed — not yet executed</div>
                  <div className="space-y-2">
                    <div>
                      <label className="mb-1 block text-xs text-slate-400">Label</label>
                      <input
                        type="text"
                        defaultValue={node.label}
                        onBlur={(e) => {
                          if (e.target.value !== node.label) {
                            onUpdateNode(node.id, e.target.value, node.prompt);
                          }
                        }}
                        className="w-full rounded-xl border border-slate-600/70 bg-[#0f1726] px-3 py-1.5 text-sm text-slate-100 outline-none focus:border-sky-400/60"
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-xs text-slate-400">Prompt</label>
                      <textarea
                        defaultValue={node.prompt}
                        onBlur={(e) => {
                          if (e.target.value !== node.prompt) {
                            onUpdateNode(node.id, node.label, e.target.value);
                          }
                        }}
                        rows={3}
                        className="w-full resize-none rounded-xl border border-slate-600/70 bg-[#0f1726] px-3 py-1.5 text-sm text-slate-100 outline-none focus:border-sky-400/60"
                      />
                    </div>
                  </div>
                </div>
              </div>
            )}

            {(node.status === "running" || node.status === "paused") && onValidateRuntime && (
              <div className="rounded-2xl border border-amber-400/20 bg-amber-500/10 p-4">
                <div className="text-[11px] uppercase tracking-[0.18em] text-amber-200/80">
                  Runtime recovery
                </div>
                <div className="mt-2 text-sm leading-6 text-amber-50/90">
                  If this node looks stuck, validate whether the agent process is still alive. A stale
                  running state will be marked failed so you can retry cleanly.
                </div>
                <Button
                  variant="outline"
                  onClick={() => onValidateRuntime(node.id)}
                  className="mt-3 rounded-2xl border-amber-300/30 bg-amber-950/30 text-amber-50 hover:bg-amber-900/35"
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Validate session state
                </Button>
                {usesPtySessionControls(effectiveAgent) && node.status === "running" && onSendEnter && (
                  <Button
                    variant="outline"
                    onClick={() => onSendEnter(node.id)}
                    className="mt-3 rounded-2xl border-emerald-300/30 bg-emerald-950/30 text-emerald-50 hover:bg-emerald-900/35"
                  >
                    <CornerDownLeft className="mr-2 h-4 w-4" />
                    Send Enter
                  </Button>
                )}
                {onStop && (
                  <Button
                    variant="outline"
                    onClick={() => onStop(node.id)}
                    className="mt-3 rounded-2xl border-rose-300/30 bg-rose-950/30 text-rose-50 hover:bg-rose-900/35"
                  >
                    <Square className="mr-2 h-4 w-4" />
                    Stop session
                  </Button>
                )}
              </div>
            )}

            {/* Resolution summary card */}
            {isResolutionNode && children.length > 0 && (
              <div className="rounded-2xl border border-violet-400/20 bg-violet-500/10 p-4">
                <div className="text-sm font-medium text-violet-100">
                  {visualType === "synthesis" ? "Synthesis review" : "Compare review"}
                </div>
                <div className="mt-2 text-sm leading-6 text-slate-300">
                  {visualType === "synthesis"
                    ? "Combine useful work from sibling branches into one stronger result."
                    : "Compare branches and choose the single best result."}
                </div>
                <div className="mt-4 grid gap-2 text-sm">
                  {children.map((child) => (
                    <div
                      key={child.id}
                      className="flex items-center justify-between rounded-xl border border-slate-700/70 bg-[#182235] px-3 py-2"
                    >
                      <span className="text-slate-200 truncate">
                        {child.label}
                      </span>
                      <span
                        className={
                          child.status === "completed"
                            ? "text-emerald-200"
                            : child.status === "running"
                              ? "text-amber-200"
                              : "text-slate-400"
                        }
                      >
                        {statusLabels[child.status] ?? child.status}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Overview content */}
            <div className="rounded-2xl border border-slate-700/70 bg-[#182235] p-4 text-sm leading-6 text-slate-300">
              {node.prompt || "No details available."}
            </div>

            {/* Metadata */}
            {((!isStructural && node.branch_name) || node.commit_hash) && (
              <div className="space-y-2">
                {!isStructural && node.branch_name && (
                  <div className="flex items-center gap-3 rounded-2xl border border-slate-700/70 bg-[#182235] px-3 py-2 text-sm text-slate-200">
                    <ScrollText className="h-4 w-4 text-slate-400" />
                    <span className="font-mono text-xs">{node.branch_name}</span>
                  </div>
                )}
                {node.commit_hash && (
                  <div className="flex items-center gap-3 rounded-2xl border border-slate-700/70 bg-[#182235] px-3 py-2 text-sm text-slate-200">
                    <ScrollText className="h-4 w-4 text-slate-400" />
                    <span className="font-mono text-xs">
                      {node.commit_hash.slice(0, 7)}
                    </span>
                  </div>
                )}
              </div>
            )}

            {/* Timestamps */}
            <div className="grid grid-cols-2 gap-3 text-sm">
              <div className="rounded-2xl border border-slate-700/70 bg-[#182235] p-3">
                <div className="text-xs text-slate-400">Created</div>
                <div className="mt-1 text-slate-100">
                  {formatRelativeTime(node.created_at)}
                </div>
              </div>
              <div className="rounded-2xl border border-slate-700/70 bg-[#182235] p-3">
                <div className="text-xs text-slate-400">
                  {isSessionRoot ? "Duration" : "Updated"}
                </div>
                <div className="mt-1 text-slate-100">
                  {isSessionRoot ? formatSessionRuntime(node) : formatRelativeTime(node.updated_at)}
                </div>
              </div>
            </div>
          </div>
        )}

        {activeTab === "Actions" && (
          <div className="absolute inset-0 overflow-y-auto p-6">
            <div className="grid gap-3">
              {/* Task (pending or terminal): Add decision, Add agent */}
              {visualType === "task" && (isTerminal || node.status === "pending") && (
                <>
                  {flowMode !== "linear" && (
                    <Button
                      variant="outline"
                      onClick={() => onCreateStructuralNode?.(node.id, "decision")}
                      className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                    >
                      <GitFork className="mr-2 h-4 w-4" />
                      Add decision
                    </Button>
                  )}
                  <Button
                    onClick={() => onCreateStructuralNode?.(node.id, "agent")}
                    className="justify-start rounded-2xl bg-slate-100 text-slate-950 hover:bg-slate-200"
                  >
                    <Plus className="mr-2 h-4 w-4" />
                    Add work step
                  </Button>
                </>
              )}
              {/* Decision: Add agent */}
              {visualType === "decision" && (
                <Button
                  onClick={() => onCreateStructuralNode?.(node.id, "agent")}
                  className="justify-start rounded-2xl bg-slate-100 text-slate-950 hover:bg-slate-200"
                >
                  <Plus className="mr-2 h-4 w-4" />
                  Add work step
                </Button>
              )}
              {/* Agent (pending or terminal): Add agent, Add review, Merge */}
              {visualType === "agent" && (isTerminal || node.status === "pending") && (
                <>
                  <Button
                    onClick={() => onCreateStructuralNode?.(node.id, "agent")}
                    className="justify-start rounded-2xl bg-slate-100 text-slate-950 hover:bg-slate-200"
                  >
                    <Plus className="mr-2 h-4 w-4" />
                    Add work step
                  </Button>
                  {flowMode !== "linear" && (
                    <>
                      <Button
                        variant="outline"
                        onClick={() => onCreateStructuralNode?.(node.id, "merge")}
                        className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                      >
                        <GitMerge className="mr-2 h-4 w-4" />
                        Add compare step
                      </Button>
                      <Button
                        variant="outline"
                        onClick={() => onCreateStructuralNode?.(node.id, "synthesis")}
                        className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                      >
                        <Combine className="mr-2 h-4 w-4" />
                        Add synthesize step
                      </Button>
                    </>
                  )}
                  {node.status === "completed" && (
                    <Button
                      variant="outline"
                      onClick={() => onMerge(node.id)}
                      className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                    >
                      <GitMerge className="mr-2 h-4 w-4" />
                      Merge into main
                    </Button>
                  )}
                </>
              )}
              {/* Resolution (pending): Add final */}
              {(visualType === "merge" || visualType === "synthesis") && node.status === "pending" && (
                <Button
                  variant="outline"
                  onClick={() => onCreateStructuralNode?.(node.id, "final")}
                  className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                >
                  <Trophy className="mr-2 h-4 w-4" />
                  Add finish step
                </Button>
              )}
              {["task", "agent", "merge", "synthesis", "final"].includes(visualType) && (
                <Button
                  variant="outline"
                  onClick={() => onCreateStructuralNode?.(node.id, "validation")}
                  className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                >
                  <CheckCircle2 className="mr-2 h-4 w-4" />
                  Add validation step
                </Button>
              )}
              {visualType === "validation" && (
                <Button
                  onClick={() => onCreateStructuralNode?.(node.id, "agent")}
                  className="justify-start rounded-2xl bg-slate-100 text-slate-950 hover:bg-slate-200"
                >
                  <Plus className="mr-2 h-4 w-4" />
                  Add corrective work step
                </Button>
              )}
              {/* Run pending nodes */}
              {visualType !== "decision" &&
                node.status === "pending" &&
                !node.worktree_path &&
                onRunNode && (
                  <Button
                    onClick={() => onRunNode(node.id)}
                    className="justify-start rounded-2xl bg-emerald-600 text-white hover:bg-emerald-500"
                  >
                    <Play className="mr-2 h-4 w-4" />
                    Run node
                  </Button>
                )}
              {/* Universal: Pause / Resume / Delete */}
              {onOpenTerminal && (
                <Button
                  variant="outline"
                  onClick={() => onOpenTerminal(node.id)}
                  className="justify-start rounded-2xl border-sky-400/20 bg-sky-500/10 text-sky-100 hover:bg-sky-500/20"
                >
                  <SquareTerminal className="mr-2 h-4 w-4" />
                  {node.worktree_path ? "Open agent in worktree" : "Open agent on repo"}
                </Button>
              )}
              {canPause && (
                <Button
                  variant="outline"
                  onClick={() => onPause?.(node.id)}
                  className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                >
                  <Pause className="mr-2 h-4 w-4" />
                  Pause session
                </Button>
              )}
              {canResume && (
                <Button
                  variant="outline"
                  onClick={() => onResume?.(node.id)}
                  className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                >
                  <Play className="mr-2 h-4 w-4" />
                  Continue session
                </Button>
              )}
              {(node.status === "running" || node.status === "paused") && onValidateRuntime && (
                <Button
                  variant="outline"
                  onClick={() => onValidateRuntime(node.id)}
                  className="justify-start rounded-2xl border-amber-400/20 bg-amber-500/10 text-amber-100 hover:bg-amber-500/20"
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Validate session state
                </Button>
              )}
              {usesPtySessionControls(effectiveAgent) && node.status === "running" && onSendEnter && (
                <Button
                  variant="outline"
                  onClick={() => onSendEnter(node.id)}
                  className="justify-start rounded-2xl border-emerald-400/20 bg-emerald-500/10 text-emerald-100 hover:bg-emerald-500/20"
                >
                  <CornerDownLeft className="mr-2 h-4 w-4" />
                  Send Enter
                </Button>
              )}
              {(node.status === "running" || node.status === "paused") && onStop && (
                <Button
                  variant="outline"
                  onClick={() => onStop(node.id)}
                  className="justify-start rounded-2xl border-rose-400/20 bg-rose-500/10 text-rose-100 hover:bg-rose-500/20"
                >
                  <Square className="mr-2 h-4 w-4" />
                  Stop session
                </Button>
              )}
              {(node.status === "failed" || node.status === "completed") && onRetryNode && (
                <Button
                  variant="outline"
                  onClick={() => onRetryNode(node.id)}
                  className="justify-start rounded-2xl border-emerald-400/20 bg-emerald-500/10 text-emerald-100 hover:bg-emerald-500/20"
                >
                  <RotateCcw className="mr-2 h-4 w-4" />
                  Retry from scratch
                </Button>
              )}
              {(node.status === "failed" || node.status === "completed") && onResetNode && (
                <Button
                  variant="outline"
                  onClick={() => onResetNode(node.id)}
                  className="justify-start rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                >
                  Reset to pending
                </Button>
              )}
              {canDelete && (
                <Button
                  variant="outline"
                  onClick={() => onDelete?.(node.id)}
                  className="justify-start rounded-2xl border-rose-400/20 bg-rose-500/5 text-rose-200 hover:bg-rose-500/10"
                >
                  <Trash2 className="mr-2 h-4 w-4" />
                  Delete branch
                </Button>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
