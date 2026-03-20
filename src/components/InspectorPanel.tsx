import { useState } from "react";
import {
  GitFork,
  GitMerge,
  Pause,
  Play,
  Trash2,
  ScrollText,
  Trophy,
  Plus,
} from "lucide-react";
import type { Agent, DecisionNode } from "../types";
import type { VisualNodeType } from "../types/node-types";
import { getNodeTypeMeta, inferNodeType } from "../lib/node-type-inference";
import { formatRelativeTime } from "../lib/utils";
import { Button } from "./ui/button";
import { SessionTab } from "./inspector/SessionTab";

type InspectorTab = "Overview" | "Session" | "Actions";

interface InspectorPanelProps {
  node: DecisionNode;
  agent: Agent;
  allNodes: DecisionNode[];
  onClose: () => void;
  onFork: (nodeId: string) => void;
  onMerge: (nodeId: string) => void;
  onCreateStructuralNode?: (parentId: string | null, nodeType: "task" | "decision" | "agent" | "merge" | "final") => void;
  onPause?: (nodeId: string) => void;
  onResume?: (nodeId: string) => void;
  onDelete?: (nodeId: string) => void;
  onRunNode?: (nodeId: string) => void;
  onUpdateNode?: (nodeId: string, label: string, prompt: string) => void;
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
  agent,
  allNodes,
  onClose: _onClose,
  onFork: _onFork,
  onMerge,
  onCreateStructuralNode,
  onPause,
  onResume,
  onDelete,
  onRunNode,
  onUpdateNode,
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
  const isStructural = ["decision", "merge", "final"].includes(visualType);

  const tabs: InspectorTab[] = ["Overview", "Session", "Actions"];

  const children = allNodes.filter((n) => n.parent_id === node.id);
  const isMergeNode = visualType === "merge" || node.status === "merged";

  return (
    <div className="flex h-full flex-col rounded-[1.75rem] border border-white/10 bg-white/[0.03] shadow-xl overflow-hidden">
      {/* Header */}
      <div className="border-b border-white/10 px-6 pt-5 pb-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[11px] uppercase tracking-[0.22em] text-slate-500">
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
          <div className="rounded-2xl border border-white/10 bg-black/20 p-3">
            <div className="text-xs text-slate-500">Type</div>
            <div className="mt-1 text-slate-100">{typeMeta.label}</div>
          </div>
          <div className="rounded-2xl border border-white/10 bg-black/20 p-3">
            <div className="text-xs text-slate-500">Project</div>
            <div className="mt-1 text-slate-100">{agent.name}</div>
          </div>
        </div>
      </div>

      {/* Tab bar — pill style */}
      <div className="border-b border-white/10 px-6 py-3">
        <div className="flex gap-2">
          {tabs.map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`rounded-full px-3 py-1.5 text-sm transition-colors ${
                activeTab === tab
                  ? "bg-slate-100 text-slate-950"
                  : "bg-white/5 text-slate-300 hover:bg-white/10"
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
          agentType={agent.agent_type}
          isActive={activeTab === "Session"}
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
                      <label className="text-xs text-slate-500 mb-1 block">Label</label>
                      <input
                        type="text"
                        defaultValue={node.label}
                        onBlur={(e) => {
                          if (e.target.value !== node.label) {
                            onUpdateNode(node.id, e.target.value, node.prompt);
                          }
                        }}
                        className="w-full rounded-xl border border-white/10 bg-black/30 px-3 py-1.5 text-sm text-slate-100 outline-none focus:border-sky-400/40"
                      />
                    </div>
                    <div>
                      <label className="text-xs text-slate-500 mb-1 block">Prompt</label>
                      <textarea
                        defaultValue={node.prompt}
                        onBlur={(e) => {
                          if (e.target.value !== node.prompt) {
                            onUpdateNode(node.id, node.label, e.target.value);
                          }
                        }}
                        rows={3}
                        className="w-full rounded-xl border border-white/10 bg-black/30 px-3 py-1.5 text-sm text-slate-100 outline-none focus:border-sky-400/40 resize-none"
                      />
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* Merge summary card */}
            {isMergeNode && children.length > 0 && (
              <div className="rounded-2xl border border-violet-400/20 bg-violet-500/10 p-4">
                <div className="text-sm font-medium text-violet-100">
                  Merge review
                </div>
                <div className="mt-2 text-sm leading-6 text-slate-300">
                  Compare branches and choose the winner or combine the best
                  parts.
                </div>
                <div className="mt-4 grid gap-2 text-sm">
                  {children.map((child) => (
                    <div
                      key={child.id}
                      className="flex items-center justify-between rounded-xl border border-white/10 bg-black/20 px-3 py-2"
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
            <div className="rounded-2xl border border-white/10 bg-black/20 p-4 text-sm leading-6 text-slate-300">
              {node.prompt || "No details available."}
            </div>

            {/* Metadata */}
            {((!isStructural && node.branch_name) || node.commit_hash) && (
              <div className="space-y-2">
                {!isStructural && node.branch_name && (
                  <div className="flex items-center gap-3 rounded-2xl border border-white/10 bg-black/20 px-3 py-2 text-sm text-slate-200">
                    <ScrollText className="h-4 w-4 text-slate-500" />
                    <span className="font-mono text-xs">{node.branch_name}</span>
                  </div>
                )}
                {node.commit_hash && (
                  <div className="flex items-center gap-3 rounded-2xl border border-white/10 bg-black/20 px-3 py-2 text-sm text-slate-200">
                    <ScrollText className="h-4 w-4 text-slate-500" />
                    <span className="font-mono text-xs">
                      {node.commit_hash.slice(0, 7)}
                    </span>
                  </div>
                )}
              </div>
            )}

            {/* Timestamps */}
            <div className="grid grid-cols-2 gap-3 text-sm">
              <div className="rounded-2xl border border-white/10 bg-black/20 p-3">
                <div className="text-xs text-slate-500">Created</div>
                <div className="mt-1 text-slate-100">
                  {formatRelativeTime(node.created_at)}
                </div>
              </div>
              <div className="rounded-2xl border border-white/10 bg-black/20 p-3">
                <div className="text-xs text-slate-500">Updated</div>
                <div className="mt-1 text-slate-100">
                  {formatRelativeTime(node.updated_at)}
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
                  <Button
                    variant="outline"
                    onClick={() => onCreateStructuralNode?.(node.id, "decision")}
                    className="justify-start rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                  >
                    <GitFork className="mr-2 h-4 w-4" />
                    Add decision point
                  </Button>
                  <Button
                    onClick={() => onCreateStructuralNode?.(node.id, "agent")}
                    className="justify-start rounded-2xl bg-slate-100 text-slate-950 hover:bg-slate-200"
                  >
                    <Plus className="mr-2 h-4 w-4" />
                    Add agent node
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
                  Add agent node
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
                    Add agent node
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => onCreateStructuralNode?.(node.id, "merge")}
                    className="justify-start rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                  >
                    <GitMerge className="mr-2 h-4 w-4" />
                    Add review step
                  </Button>
                  {node.status === "completed" && (
                    <Button
                      variant="outline"
                      onClick={() => onMerge(node.id)}
                      className="justify-start rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                    >
                      <GitMerge className="mr-2 h-4 w-4" />
                      Merge into main
                    </Button>
                  )}
                </>
              )}
              {/* Merge (pending): Add final */}
              {visualType === "merge" && node.status === "pending" && (
                <Button
                  variant="outline"
                  onClick={() => onCreateStructuralNode?.(node.id, "final")}
                  className="justify-start rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                >
                  <Trophy className="mr-2 h-4 w-4" />
                  Add final output
                </Button>
              )}
              {/* Run pending nodes */}
              {(visualType === "task" || visualType === "agent") &&
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
              {canPause && (
                <Button
                  variant="outline"
                  onClick={() => onPause?.(node.id)}
                  className="justify-start rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                >
                  <Pause className="mr-2 h-4 w-4" />
                  Pause session
                </Button>
              )}
              {canResume && (
                <Button
                  variant="outline"
                  onClick={() => onResume?.(node.id)}
                  className="justify-start rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                >
                  <Play className="mr-2 h-4 w-4" />
                  Resume session
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
