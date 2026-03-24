import { useEffect, useState } from "react";
import {
  Circle,
  CheckCircle2,
  XCircle,
  Loader2,
  Pause,
  Clock,
  Play,
  RefreshCw,
  CornerDownLeft,
  Square,
} from "lucide-react";
import type { AgentType, DecisionNode, OrchestratorStatus } from "../types";
import { usesPtySessionControls, usesStructuredSession } from "../lib/agent-runtime";
import { SdkSessionView } from "./SdkSessionView";
import { TerminalView } from "./TerminalView";
import { Button } from "./ui/button";

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m < 60) return `${m}m ${s}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

function useLiveClock(hasRunning: boolean): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    if (!hasRunning) return;
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(id);
  }, [hasRunning]);
  return now;
}

function getNodeElapsedSeconds(node: DecisionNode, now: number): number | null {
  if (node.status === "pending") return null;

  const start = node.started_at ?? (node.status === "running" ? node.updated_at : node.created_at);
  if (node.status === "running") {
    return Math.max(0, now - start);
  }

  return Math.max(0, node.updated_at - start);
}

interface OrchestratorActivityProps {
  agentType: AgentType;
  treeNodes: DecisionNode[];
  orchestratorStatus: OrchestratorStatus;
  onSelectNode: (id: string) => void;
  onValidateRuntime?: (nodeId: string) => void;
  onSendEnter?: (nodeId: string) => void;
  onResumeNode?: (nodeId: string) => void;
  onStopNode?: (nodeId: string) => void;
}

const statusIcon: Record<string, React.ReactNode> = {
  pending: <Clock className="size-3 text-slate-500" />,
  running: <Loader2 className="size-3 text-amber-400 animate-spin" />,
  paused: <Pause className="size-3 text-sky-400" />,
  completed: <CheckCircle2 className="size-3 text-emerald-400" />,
  failed: <XCircle className="size-3 text-rose-400" />,
  merged: <Circle className="size-3 text-violet-400" />,
};

const statusColor: Record<string, string> = {
  pending: "text-slate-500",
  running: "text-amber-300",
  paused: "text-sky-300",
  completed: "text-emerald-300",
  failed: "text-rose-300",
  merged: "text-violet-300",
};

export function OrchestratorActivity({
  agentType,
  treeNodes,
  orchestratorStatus,
  onSelectNode,
  onValidateRuntime,
  onSendEnter,
  onResumeNode,
  onStopNode,
}: OrchestratorActivityProps) {
  // Auto-follow the currently running node
  const [viewingNodeId, setViewingNodeId] = useState<string | null>(
    orchestratorStatus.current_node_id,
  );

  // When orchestrator moves to a new node, auto-follow it
  useEffect(() => {
    if (orchestratorStatus.current_node_id) {
      setViewingNodeId(orchestratorStatus.current_node_id);
    }
  }, [orchestratorStatus.current_node_id]);

  const viewingNode = viewingNodeId
    ? treeNodes.find((n) => n.id === viewingNodeId)
    : null;

  // Show all runnable nodes — skip only decision nodes (pure branching points)
  const runnableNodes = treeNodes.filter(
    (n) => n.node_type !== "decision",
  );

  const hasRunning = runnableNodes.some((n) => n.status === "running");
  const now = useLiveClock(hasRunning);

  const progress = orchestratorStatus.total_count > 0
    ? Math.round((orchestratorStatus.completed_count / orchestratorStatus.total_count) * 100)
    : 0;

  const canValidate = viewingNode?.status === "running" || viewingNode?.status === "paused";
  const canSendEnter = usesPtySessionControls(agentType) && viewingNode?.status === "running";
  const canContinue = viewingNode?.status === "paused";
  const canStop = viewingNode?.status === "running" || viewingNode?.status === "paused";

  return (
    <div className="flex h-full flex-col rounded-[1.75rem] border border-white/10 bg-white/[0.03] shadow-xl overflow-hidden">
      {/* Header */}
      <div className="border-b border-white/10 px-5 pt-4 pb-3">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-[11px] uppercase tracking-[0.22em] text-slate-500">
              Orchestrator
            </div>
            <div className="mt-1 text-sm font-medium text-slate-100">
              {orchestratorStatus.state === "waiting_user"
                ? "Awaiting decision"
                : orchestratorStatus.state === "complete"
                  ? "Complete"
                  : orchestratorStatus.state === "failed"
                    ? "Failed"
                    : "Running"}
            </div>
          </div>
          <div className="text-right">
            <div className="text-lg font-semibold tabular-nums text-slate-100">
              {orchestratorStatus.completed_count}/{orchestratorStatus.total_count}
            </div>
            <div className="text-[11px] text-slate-500">nodes done</div>
          </div>
        </div>
        {/* Progress bar */}
        <div className="mt-3 h-1 rounded-full bg-white/5 overflow-hidden">
          <div
            className="h-full rounded-full bg-emerald-500 transition-all duration-500 ease-out"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>

      {/* Node list — compact scrollable */}
      <div className="border-b border-white/10 max-h-[200px] overflow-y-auto">
        {runnableNodes.map((node) => {
          const isCurrent = node.id === orchestratorStatus.current_node_id;
          const isViewing = node.id === viewingNodeId;

          return (
            <button
              key={node.id}
              onClick={() => {
                setViewingNodeId(node.id);
                onSelectNode(node.id);
              }}
              className={`flex w-full items-center gap-2.5 px-5 py-2 text-left text-sm transition-colors ${
                isViewing
                  ? "bg-white/10"
                  : isCurrent
                    ? "bg-amber-500/5"
                    : "hover:bg-white/5"
              }`}
            >
              {statusIcon[node.status] ?? statusIcon.pending}
              <span
                className={`min-w-0 flex-1 truncate ${
                  isCurrent ? "text-slate-100 font-medium" : "text-slate-400"
                }`}
              >
                {node.label}
              </span>
              <span className={`text-[11px] tabular-nums ${statusColor[node.status] ?? "text-slate-500"}`}>
                {(() => {
                  const elapsed = getNodeElapsedSeconds(node, now);
                  return elapsed == null ? node.status : formatDuration(elapsed);
                })()}
              </span>
            </button>
          );
        })}
      </div>

      {/* Terminal output for the selected/current node */}
      <div className="flex-1 min-h-0 min-w-0 overflow-hidden">
        {viewingNode ? (
          <div className="flex h-full min-w-0 flex-col">
            <div className="flex items-center gap-2 border-b border-white/10 px-5 py-2">
              <div className="size-2 rounded-full bg-amber-400 animate-pulse" style={{
                animationPlayState: viewingNode.status === "running" ? "running" : "paused",
                backgroundColor: viewingNode.status === "completed" ? "#34d399"
                  : viewingNode.status === "failed" ? "#fb7185"
                  : viewingNode.status === "running" ? "#fbbf24"
                  : "#64748b",
              }} />
              <span className="text-xs text-slate-300 truncate font-medium">
                {viewingNode.label}
              </span>
              <span className="ml-auto text-[10px] text-slate-500 font-mono">
                {viewingNode.id.slice(0, 8)}
              </span>
            </div>
            <div className="border-b border-white/10 bg-black/10 px-5 py-3">
              <div className="flex flex-wrap items-center gap-2">
                {canValidate && onValidateRuntime && (
                  <Button
                    variant="outline"
                    onClick={() => onValidateRuntime(viewingNode.id)}
                    className="rounded-2xl border-amber-400/20 bg-amber-500/10 text-amber-100 hover:bg-amber-500/20"
                  >
                    <RefreshCw className="mr-2 h-4 w-4" />
                    Validate state
                  </Button>
                )}
                {canSendEnter && onSendEnter && (
                  <Button
                    variant="outline"
                    onClick={() => onSendEnter(viewingNode.id)}
                    className="rounded-2xl border-emerald-400/20 bg-emerald-500/10 text-emerald-100 hover:bg-emerald-500/20"
                  >
                    <CornerDownLeft className="mr-2 h-4 w-4" />
                    Send Enter
                  </Button>
                )}
                {canContinue && onResumeNode && (
                  <Button
                    variant="outline"
                    onClick={() => onResumeNode(viewingNode.id)}
                    className="rounded-2xl border-sky-400/20 bg-sky-500/10 text-sky-100 hover:bg-sky-500/20"
                  >
                    <Play className="mr-2 h-4 w-4" />
                    Continue session
                  </Button>
                )}
                {canStop && onStopNode && (
                  <Button
                    variant="outline"
                    onClick={() => onStopNode(viewingNode.id)}
                    className="rounded-2xl border-rose-400/20 bg-rose-500/10 text-rose-100 hover:bg-rose-500/20"
                  >
                    <Square className="mr-2 h-4 w-4" />
                    Stop session
                  </Button>
                )}
              </div>
              {(viewingNode.status === "running" || viewingNode.status === "paused") && (
                <div className="mt-2 text-[11px] leading-5 text-slate-400">
                  If this looks stuck, validate the runtime state. Terminal-backed agents can still
                  accept a manual Enter, and Stop session always gives you a clean retry path.
                </div>
              )}
            </div>
            <div className="flex-1 min-h-0 min-w-0">
              {usesStructuredSession(agentType) ? (
                <SdkSessionView sessionId={viewingNode.id} status={viewingNode.status} />
              ) : (
                <TerminalView sessionId={viewingNode.id} status={viewingNode.status} />
              )}
            </div>
          </div>
        ) : (
          <div className="flex h-full items-center justify-center">
            <span className="text-xs text-slate-500">
              Waiting for first node to start...
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
