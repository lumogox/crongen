import { memo } from "react";
import { Handle, Position } from "@xyflow/react";
import {
  Clock3,
  Play,
  CheckCircle2,
  XCircle,
  Pause,
  GitMerge as MergeIcon,
  X,
  RotateCcw,
  SquareTerminal,
} from "lucide-react";
import type { AgentType, DecisionNodeData, NodeStatus } from "../types";
import { getAgentLabel } from "../lib/agent-templates";
import { getNodeTypeMeta } from "../lib/node-type-inference";

const statusConfig: Record<
  NodeStatus,
  { icon: React.ElementType; label: string; tone: string }
> = {
  pending: {
    icon: Clock3,
    label: "Queued",
    tone: "bg-slate-500/10 text-slate-300 border-slate-400/20",
  },
  running: {
    icon: Play,
    label: "Running",
    tone: "bg-amber-500/10 text-amber-200 border-amber-400/20",
  },
  paused: {
    icon: Pause,
    label: "Paused",
    tone: "bg-sky-500/10 text-sky-200 border-sky-400/20",
  },
  completed: {
    icon: CheckCircle2,
    label: "Succeeded",
    tone: "bg-emerald-500/10 text-emerald-200 border-emerald-400/20",
  },
  failed: {
    icon: XCircle,
    label: "Failed",
    tone: "bg-rose-500/10 text-rose-200 border-rose-400/20",
  },
  merged: {
    icon: MergeIcon,
    label: "Merged",
    tone: "bg-violet-500/10 text-violet-200 border-violet-400/20",
  },
};

const structuralTypes = new Set(["decision", "merge", "final"]);
const assignableAgents: AgentType[] = ["claude_code", "codex", "gemini"];

function ExecutionNodeInner({
  data,
  selected,
}: {
  data: DecisionNodeData;
  id: string;
  type: string;
  selected?: boolean;
}) {
  const {
    node,
    visualType,
    onFork: _onFork,
    onMerge,
    onRunNode,
    onUpdateNodeAgent,
    onDeleteNode,
    onOpenNodeTerminal,
    defaultExecutionAgent,
    isOrchestratorTarget,
    debugMode,
    onResetNode,
  } = data;
  const typeMeta = getNodeTypeMeta(visualType);
  const TypeIcon = typeMeta.icon;
  const isStructural = structuralTypes.has(visualType);
  const effectiveAgent = node.agent_type_override ?? defaultExecutionAgent;
  const canAssignAgent =
    ["task", "agent", "merge", "final"].includes(visualType) &&
    node.status === "pending" &&
    !node.worktree_path;

  // For structural nodes with pending status, show "Needs review"
  const status = { ...statusConfig[node.status] };
  if (isStructural && node.status === "pending") {
    status.label = "Needs review";
  }
  const StatusIcon = status.icon;

  const shortHash = node.commit_hash?.slice(0, 7);
  const showBranch = visualType !== "decision";
  const shortBranch =
    showBranch && node.branch_name && node.branch_name.length > 18
      ? node.branch_name.slice(0, 18) + "..."
      : showBranch
        ? node.branch_name
        : null;

  // Context-sensitive action rules per node type
  const isPending = node.status === "pending";
  const actions: { label: string; icon: React.ElementType; tone: string; onClick: () => void }[] = [];

  // Run button for any pending node (not yet executed)
  if (isPending && !node.worktree_path && visualType !== "decision") {
    actions.push({
      label: "Run",
      icon: Play,
      tone: "border-emerald-400/30 bg-emerald-500/10 text-emerald-200 hover:bg-emerald-500/20 hover:border-emerald-400/50",
      onClick: () => onRunNode(node.id),
    });
  }

  if (visualType === "agent" && node.status === "completed") {
    actions.push({
      label: "Merge",
      icon: MergeIcon,
      tone: "border-violet-400/30 bg-violet-500/10 text-violet-200 hover:bg-violet-500/20 hover:border-violet-400/50",
      onClick: () => onMerge(node.id),
    });
  }
  // Debug mode: Reset button for non-pending nodes
  if (debugMode && onResetNode && node.status !== "pending") {
    actions.push({
      label: "Reset",
      icon: RotateCcw,
      tone: "border-slate-400/30 bg-slate-500/10 text-slate-300 hover:bg-slate-500/20 hover:border-slate-400/50",
      onClick: () => onResetNode(node.id),
    });
  }

  // "final" type has no actions

  const canDelete = node.status !== "running";

  return (
    <div
      className={`group/node w-[250px] rounded-2xl border shadow-xl backdrop-blur ${
        isOrchestratorTarget
          ? "border-sky-400/50 ring-2 ring-sky-400/30 animate-pulse"
          : selected
            ? "border-sky-400/40 ring-1 ring-sky-400/40"
            : "border-slate-700/70"
      }`}
      style={{ backgroundColor: "rgba(18, 26, 42, 0.96)" }}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-3 !w-3 !border-2 !border-slate-900 !bg-slate-400"
      />

      {/* Delete button — top-right, visible on hover */}
      {canDelete && (
        <button
          onClick={(e) => { e.stopPropagation(); onDeleteNode(node.id); }}
          className="absolute -right-2 -top-2 z-10 flex h-5 w-5 items-center justify-center rounded-full border border-slate-600/70 bg-[#182235] text-slate-400 opacity-0 transition-opacity hover:border-rose-400/40 hover:bg-rose-500/20 hover:text-rose-300 group-hover/node:opacity-100"
          title="Delete node"
        >
          <X className="h-3 w-3" />
        </button>
      )}

      <div className="p-4">
        {/* Top row: type label + status pill */}
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="mb-1 flex items-center gap-2 text-[11px] uppercase tracking-[0.18em] text-slate-400">
              <TypeIcon className="h-3.5 w-3.5" />
              <span>{typeMeta.label}</span>
            </div>
            <div className="truncate text-base font-semibold text-slate-50">
              {node.label}
            </div>
          </div>

          <div className="flex shrink-0 items-start gap-2">
            {onOpenNodeTerminal && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  onOpenNodeTerminal(node.id);
                }}
                className={`inline-flex h-8 w-8 items-center justify-center rounded-full border border-sky-400/20 bg-sky-500/10 text-sky-200 transition-all hover:bg-sky-500/20 hover:text-sky-100 focus-visible:opacity-100 ${
                  selected
                    ? "opacity-100"
                    : "opacity-0 group-hover/node:opacity-100"
                }`}
                title={node.worktree_path ? "Open agent terminal in worktree" : "Open agent terminal on repo"}
              >
                <SquareTerminal className="h-3.5 w-3.5" />
              </button>
            )}
            <span
              className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2 py-1 text-[11px] font-medium ${status.tone}`}
            >
              <StatusIcon className="h-3.5 w-3.5" />
              {status.label}
            </span>
          </div>
        </div>

        {/* Subtitle */}
        <div className="mt-3 truncate text-sm text-slate-300">
          {node.prompt}
        </div>

        {/* Execution indicator */}
        {typeMeta.summary ? (
          <div className="mt-1 text-xs text-slate-400">{typeMeta.summary}</div>
        ) : shortBranch ? (
          <div className="mt-1 text-xs text-slate-400">{shortBranch}</div>
        ) : null}

        {/* Tags row — hide for structural nodes without data */}
        {(shortHash || node.exit_code != null) && (
          <div className="mt-4 flex flex-wrap gap-2 text-[11px]">
            {shortHash && (
              <span className="rounded-full border border-slate-600/70 bg-[#182235] px-2 py-1 text-slate-300">
                {shortHash}
              </span>
            )}
            {node.exit_code != null && (
              <span
                className={`rounded-full border px-2 py-1 ${
                  node.exit_code === 0
                    ? "border-emerald-400/20 bg-emerald-500/10 text-emerald-100"
                    : "border-rose-400/20 bg-rose-500/10 text-rose-100"
                }`}
              >
                Exit {node.exit_code}
              </span>
            )}
          </div>
        )}
      </div>

      {/* Inline action bar */}
      {(actions.length > 0 || canAssignAgent) && (
        <div className="flex flex-wrap items-center gap-1.5 border-t border-slate-700/70 px-3 py-2">
          {canAssignAgent && (
            <label
              className="flex min-w-0 flex-1 items-center gap-1.5 rounded-full border border-slate-600/70 bg-[#0f1726] px-2.5 py-1 text-[11px] text-slate-300"
              title={`Execution agent${node.agent_type_override ? "" : `: default${effectiveAgent ? ` (${getAgentLabel(effectiveAgent)})` : ""}`}`}
              onClick={(e) => e.stopPropagation()}
              onPointerDown={(e) => e.stopPropagation()}
            >
              <span className="shrink-0 text-slate-500">Agent</span>
              <select
                value={node.agent_type_override ?? ""}
                onChange={(e) => {
                  const nextAgent = e.target.value ? (e.target.value as AgentType) : null;
                  onUpdateNodeAgent(node.id, nextAgent);
                }}
                className="nodrag min-w-0 flex-1 appearance-none bg-transparent text-slate-100 outline-none"
                aria-label="Execution agent"
              >
                <option value="">
                  {effectiveAgent ? `Default: ${getAgentLabel(effectiveAgent)}` : "Default"}
                </option>
                {assignableAgents.map((agent) => (
                  <option key={agent} value={agent}>
                    {getAgentLabel(agent)}
                  </option>
                ))}
              </select>
            </label>
          )}
          {actions.map((action) => {
            const Icon = action.icon;
            return (
              <button
                key={action.label}
                onClick={(e) => {
                  e.stopPropagation();
                  action.onClick();
                }}
                className={`flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-all ${action.tone}`}
              >
                <Icon className="h-3 w-3" />
                {action.label}
              </button>
            );
          })}
        </div>
      )}

      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-3 !w-3 !border-2 !border-slate-900 !bg-sky-400"
      />
    </div>
  );
}

export const ExecutionNode = memo(ExecutionNodeInner);
