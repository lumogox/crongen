import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { Sparkles, GitFork, GitMerge, Play, PlayCircle, XCircle, Loader2, Settings, FileCode, CheckCircle2, Rocket } from "lucide-react";
import type { AgentType, DecisionNode, OrchestratorMode, OrchestratorStatus, Project } from "../types";
import type { StructuralNodeType } from "../types/node-types";
import { DecisionCanvas } from "./DecisionCanvas";
import { InspectorPanel } from "./InspectorPanel";
import { OrchestratorActivity } from "./OrchestratorActivity";
import { CanvasToolbar } from "./CanvasToolbar";
import { Sidebar } from "./Sidebar";
import { Button } from "./ui/button";
import { DndProvider } from "./DndContext";
import { NodePalette } from "./NodePalette";
import { MergeDialog } from "./MergeDialog";
import { getNodeContext } from "../lib/tauri-commands";

interface ContentAreaProps {
  projects: Project[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onNewProject: () => void;
  onEditProject: (project: Project) => void;
  onDeleteProject: (project: Project) => void;
  selectedProject: Project | null;
  treeNodes: DecisionNode[];
  treeLoading: boolean;
  selectedNodeId: string | null;
  onSelectNode: (id: string | null) => void;
  onForkNode: (nodeId: string) => void;
  onMergeNode: (nodeId: string) => void;
  onCreateStructuralNode: (parentId: string | null, nodeType: StructuralNodeType) => void;
  onPlanFromNode: (parentId: string) => void;
  onRefinePlan: (sessionId: string) => void;
  onRunNow: (projectId: string) => void;
  onCloseTerminal: () => void;
  onPauseNode?: (nodeId: string) => void;
  onResumeNode?: (nodeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
  flowMode: "linear" | "branching";
  onFlowModeChange: (mode: "linear" | "branching") => void;
  sessions: DecisionNode[];
  selectedSessionId: string | null;
  onSelectSession: (id: string | null) => void;
  onCreateSession: () => void;
  onDeleteSession: (session: DecisionNode) => void;
  onRunNode: (nodeId: string) => void;
  onUpdateNode: (nodeId: string, label: string, prompt: string) => void;
  onUpdateNodeAgent: (nodeId: string, agentType: AgentType | null) => void;
  orchestratorStatus: OrchestratorStatus | null;
  onStartOrchestrator?: (mode: OrchestratorMode) => void;
  onCancelOrchestrator?: () => void;
  onMergeComplete?: (outcome: "merged" | "branched") => void;
  currentBranch?: string | null;
  debugMode?: boolean;
  agentSetupReminder?: string | null;
  onOpenSettings?: () => void;
  onValidateRuntime?: (nodeId: string) => void;
  onSendEnterToNode?: (nodeId: string) => void;
  onStopNode?: (nodeId: string) => void;
  onRetryNode?: (nodeId: string) => void;
  onResetNode?: (nodeId: string) => void;
  onOpenNodeTerminal?: (nodeId: string) => void;
  defaultExecutionAgent?: AgentType | null;
  manualTerminalSessionId?: string | null;
}

export function ContentArea(props: ContentAreaProps) {
  return (
    <ReactFlowProvider>
      <DndProvider>
        <ContentAreaInner {...props} />
      </DndProvider>
    </ReactFlowProvider>
  );
}

// ─── Drag-to-resize hook ──────────────────────────────────────

function useResizeHandle(
  initial: number,
  min: number,
  max: number,
  direction: "left" | "right",
) {
  const [size, setSize] = useState(initial);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startSize = useRef(initial);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;
      startX.current = e.clientX;
      startSize.current = size;

      const onMouseMove = (ev: MouseEvent) => {
        if (!dragging.current) return;
        const delta = ev.clientX - startX.current;
        const newSize = startSize.current + (direction === "left" ? delta : -delta);
        setSize(Math.max(min, Math.min(max, newSize)));
      };

      const onMouseUp = () => {
        dragging.current = false;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [size, min, max, direction],
  );

  return { size, onMouseDown };
}

function ResizeHandle({ onMouseDown }: { onMouseDown: (e: React.MouseEvent) => void }) {
  return (
    <div
      onMouseDown={onMouseDown}
      className="group flex w-2 shrink-0 cursor-col-resize items-center justify-center"
    >
      <div className="h-8 w-0.5 rounded-full bg-slate-600/60 transition-colors group-hover:bg-slate-400/80 group-active:bg-sky-400/70" />
    </div>
  );
}

// ─── Main layout ──────────────────────────────────────────────

function ContentAreaInner({
  projects,
  selectedProjectId,
  onSelectProject,
  onNewProject,
  onEditProject,
  onDeleteProject,
  selectedProject,
  treeNodes,
  selectedNodeId,
  onSelectNode,
  onForkNode,
  onMergeNode,
  onCreateStructuralNode,
  onPlanFromNode,
  onRefinePlan,
  onRunNow,
  onCloseTerminal,
  onPauseNode,
  onResumeNode,
  onDeleteNode,
  flowMode,
  onFlowModeChange,
  sessions,
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  onDeleteSession,
  onRunNode,
  onUpdateNode,
  onUpdateNodeAgent,
  orchestratorStatus,
  onStartOrchestrator,
  onCancelOrchestrator,
  onMergeComplete,
  currentBranch,
  debugMode,
  agentSetupReminder,
  onOpenSettings,
  onValidateRuntime,
  onSendEnterToNode,
  onStopNode,
  onRetryNode,
  onResetNode,
  onOpenNodeTerminal,
  defaultExecutionAgent,
  manualTerminalSessionId,
}: ContentAreaProps) {
  const selectedNode = selectedNodeId
    ? treeNodes.find((n) => n.id === selectedNodeId) ?? null
    : null;

  // Resizable panel widths
  const sidebar = useResizeHandle(260, 180, 400, "left");
  const inspector = useResizeHandle(360, 240, 550, "right");

  // Show orchestrator activity panel when orchestrator is actively running
  const showOrchestratorActivity =
    orchestratorStatus &&
    (orchestratorStatus.state === "running" || orchestratorStatus.state === "waiting_user");

  // Detect if session has pending runnable nodes remaining (can resume/continue).
  // Decision nodes are structural (never executed) so they stay "pending" forever — exclude them.
  const hasPendingNodes = useMemo(() => {
    if (!selectedSessionId || treeNodes.length === 0) return false;
    return treeNodes.some((n) => n.status === "pending" && n.node_type !== "decision");
  }, [selectedSessionId, treeNodes]);

  const hasCompletedNodes = useMemo(() => {
    return treeNodes.some((n) => n.status === "completed" || n.status === "failed");
  }, [treeNodes]);

  const orchestratorIsIdle = !orchestratorStatus ||
    orchestratorStatus.state === "complete" ||
    orchestratorStatus.state === "failed" ||
    orchestratorStatus.state === "idle";

  const canContinue = hasPendingNodes && hasCompletedNodes && orchestratorIsIdle;
  const hasSelectedSession = Boolean(selectedSessionId);
  const hasCanvasNodes = treeNodes.length > 0;

  // ─── Session completion detection ──────────────────────────
  // A session is complete when: there are nodes, no runnable nodes are pending,
  // and the orchestrator isn't actively running.
  // Session root already merged to main — don't show merge buttons
  const sessionAlreadyMerged = useMemo(() => {
    if (!selectedSessionId) return false;
    const root = treeNodes.find((n) => n.id === selectedSessionId);
    return root?.status === "merged";
  }, [selectedSessionId, treeNodes]);

  const sessionComplete = useMemo(() => {
    if (!selectedSessionId || treeNodes.length === 0) return false;
    if (sessionAlreadyMerged) return false;
    const runnableNodes = treeNodes.filter((n) => n.node_type !== "decision");
    if (runnableNodes.length === 0) return false;
    const allDone = runnableNodes.every(
      (n) => n.status === "completed" || n.status === "failed" || n.status === "merged",
    );
    return allDone && orchestratorIsIdle;
  }, [selectedSessionId, treeNodes, orchestratorIsIdle, sessionAlreadyMerged]);

  // Find the terminal node: deepest leaf that's completed/merged (the branch to merge to main).
  const terminalNodeId = useMemo(() => {
    if (!sessionComplete) return null;
    const childIds = new Set(treeNodes.map((n) => n.parent_id).filter(Boolean));
    // Leaf nodes = nodes that are NOT a parent of any other node
    const leaves = treeNodes.filter(
      (n) => !childIds.has(n.id) && (n.status === "completed" || n.status === "merged"),
    );
    if (leaves.length === 0) return null;
    // Prefer "final" type, then resolution nodes, then deepest by created_at
    const sorted = leaves.sort((a, b) => {
      const typeRank = (t: string | null) =>
        t === "final" ? 0 : t === "merge" || t === "synthesis" ? 1 : 2;
      const rankDiff = typeRank(a.node_type) - typeRank(b.node_type);
      if (rankDiff !== 0) return rankDiff;
      return b.created_at - a.created_at;
    });
    return sorted[0]?.id ?? null;
  }, [sessionComplete, treeNodes]);

  const terminalNode = useMemo(() => {
    if (!terminalNodeId) return null;
    return treeNodes.find((n) => n.id === terminalNodeId) ?? null;
  }, [terminalNodeId, treeNodes]);

  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-hidden">
      {/* Header */}
      <header className="no-select flex flex-wrap items-center justify-between gap-3 rounded-[1.5rem] border border-slate-700/70 bg-[#121a2a] px-4 py-3">
        <div className="flex items-center gap-3">
          <div>
            <div className="text-xs uppercase tracking-[0.22em] text-slate-400">
              crongen
            </div>
            <div className="mt-1 text-lg font-semibold text-slate-50">
              Execution graph
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Flow mode toggle */}
          <div className="flex items-center rounded-full border border-slate-700/70 bg-[#182235] p-0.5">
            <button
              onClick={() => onFlowModeChange("linear")}
              className={`rounded-full px-3 py-1 text-xs font-medium transition-colors ${
                flowMode === "linear"
                  ? "bg-slate-100 text-slate-950"
                  : "text-slate-300 hover:text-slate-100"
              }`}
            >
              Linear
            </button>
            <button
              onClick={() => onFlowModeChange("branching")}
              className={`rounded-full px-3 py-1 text-xs font-medium transition-colors ${
                flowMode === "branching"
                  ? "bg-slate-100 text-slate-950"
                  : "text-slate-300 hover:text-slate-100"
              }`}
            >
              Branching
            </button>
          </div>

          <Button
            variant="outline"
            disabled={!selectedProject}
            onClick={onCreateSession}
            className="rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044] disabled:opacity-40"
          >
            <Sparkles className="mr-2 h-4 w-4" />
            New task
          </Button>
          {/* Orchestrator: Run All / Progress / Cancel */}
          {selectedProject && selectedSessionId && onStartOrchestrator && (
            showOrchestratorActivity ? (
              <div className="flex items-center gap-2">
                <div className="flex items-center gap-2 rounded-2xl border border-sky-400/20 bg-sky-500/10 px-3 py-1.5 text-xs text-sky-200">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>
                    {orchestratorStatus.state === "waiting_user"
                      ? "Awaiting decision..."
                      : `Running ${treeNodes.filter((n) => n.status === "running").length}/${orchestratorStatus.total_count}`}
                  </span>
                </div>
                {onCancelOrchestrator && (
                  <Button
                    variant="outline"
                    onClick={onCancelOrchestrator}
                    className="rounded-2xl border-rose-400/20 bg-rose-500/10 text-rose-200 hover:bg-rose-500/20"
                  >
                    <XCircle className="mr-2 h-4 w-4" />
                    Cancel
                  </Button>
                )}
              </div>
            ) : sessionAlreadyMerged ? (
              <div className="flex items-center gap-2 rounded-2xl border border-violet-400/20 bg-violet-500/10 px-3 py-1.5 text-xs text-violet-200">
                <GitMerge className="h-3.5 w-3.5" />
                <span>Merged to {currentBranch ?? "main"}</span>
              </div>
            ) : sessionComplete && terminalNode ? (
              <>
                <div className="flex items-center gap-2 rounded-2xl border border-emerald-400/20 bg-emerald-500/10 px-3 py-1.5 text-xs text-emerald-200">
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  <span>Session complete</span>
                </div>
                <Button
                  onClick={() => setMergeDialogOpen(true)}
                  className="rounded-2xl bg-violet-600 text-white hover:bg-violet-500"
                >
                  <Rocket className="mr-2 h-4 w-4" />
                  Ship it
                </Button>
              </>
            ) : hasPendingNodes ? (
              <div className="flex items-center rounded-full border border-slate-700/70 bg-[#182235] p-0.5">
                {canContinue ? (
                  <>
                    <button
                      onClick={() => onStartOrchestrator("auto")}
                      className="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium text-sky-300 hover:bg-sky-500/10 transition-colors"
                    >
                      <PlayCircle className="h-3 w-3" />
                      Continue (Auto)
                    </button>
                    <button
                      onClick={() => onStartOrchestrator("supervised")}
                      className="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium text-amber-300 hover:bg-amber-500/10 transition-colors"
                    >
                      <PlayCircle className="h-3 w-3" />
                      Continue (Supervised)
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      onClick={() => onStartOrchestrator("auto")}
                      className="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium text-emerald-300 hover:bg-emerald-500/10 transition-colors"
                    >
                      <Play className="h-3 w-3" />
                      Run all (Auto)
                    </button>
                    <button
                      onClick={() => onStartOrchestrator("supervised")}
                      className="flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium text-amber-300 hover:bg-amber-500/10 transition-colors"
                    >
                      <Play className="h-3 w-3" />
                      Supervised
                    </button>
                  </>
                )}
              </div>
            ) : null
          )}

          {selectedNode && (
            <>
              <Button
                variant="outline"
                disabled={
                  !(
                    selectedNode.status === "completed" ||
                    selectedNode.status === "failed" ||
                    selectedNode.status === "paused"
                  )
                }
                onClick={() => onForkNode(selectedNode.id)}
                className="rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044] disabled:opacity-40"
              >
                <GitFork className="mr-2 h-4 w-4" />
                Branch
              </Button>
              <Button
                variant="outline"
                disabled={selectedNode.status !== "completed"}
                onClick={() => onMergeNode(selectedNode.id)}
                className="rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044] disabled:opacity-40"
              >
                <GitMerge className="mr-2 h-4 w-4" />
                Merge
              </Button>
            </>
          )}

          {/* Settings cog */}
          {onOpenSettings && (
            <button
              onClick={onOpenSettings}
              className="rounded-full p-2 text-slate-300 transition-colors hover:bg-[#243044] hover:text-slate-100"
              title="Settings"
            >
              <Settings className="h-4 w-4" />
            </button>
          )}
        </div>
      </header>

      {agentSetupReminder && onOpenSettings && (
        <div className="flex items-center justify-between gap-3 rounded-[1.35rem] border border-amber-400/20 bg-[linear-gradient(135deg,rgba(245,158,11,0.14),rgba(15,23,42,0.35))] px-4 py-3">
          <div>
            <div className="text-[11px] uppercase tracking-[0.22em] text-amber-200/80">Agent Bay</div>
            <div className="mt-1 text-sm text-amber-50">{agentSetupReminder}</div>
          </div>
          <Button
            variant="outline"
            onClick={onOpenSettings}
            className="rounded-full border-amber-300/30 bg-amber-950/30 text-amber-50 hover:bg-amber-900/35"
          >
            <Settings className="mr-2 h-4 w-4" />
            Open setup
          </Button>
        </div>
      )}

      {/* 3-column layout with drag-to-resize handles */}
      <div className="flex min-h-0 flex-1 gap-0 overflow-hidden">
        {/* Sidebar */}
        <div style={{ width: sidebar.size, minWidth: sidebar.size }} className="shrink-0">
          <Sidebar
            projects={projects}
            selectedProjectId={selectedProjectId}
            onSelectProject={onSelectProject}
            onNewProject={onNewProject}
            onEditProject={onEditProject}
            onDeleteProject={onDeleteProject}
            onRunNow={onRunNow}
            sessions={sessions}
            selectedSessionId={selectedSessionId}
            onSelectSession={onSelectSession}
            onCreateSession={onCreateSession}
            onDeleteSession={onDeleteSession}
          />
        </div>

        <ResizeHandle onMouseDown={sidebar.onMouseDown} />

        {/* Canvas column */}
        <main className="flex min-w-0 flex-1 min-h-0 flex-col overflow-hidden gap-4">
          {selectedProject ? (
            <>
              <CanvasToolbar
                title={selectedProject.name}
                subtitle={selectedProject.repo_path}
                showAutoLayout={hasSelectedSession && hasCanvasNodes}
              />
              {hasSelectedSession && hasCanvasNodes ? (
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <NodePalette
                    selectedNode={selectedNode}
                    allNodes={treeNodes}
                    onAddNode={(nodeType) => {
                      if (selectedNodeId) onCreateStructuralNode(selectedNodeId, nodeType);
                    }}
                    onPlanFromNode={() => {
                      if (selectedNodeId) onPlanFromNode(selectedNodeId);
                    }}
                  />
                  <Button
                    variant="outline"
                    onClick={() => selectedSessionId && onRefinePlan(selectedSessionId)}
                    disabled={!selectedSessionId}
                    className="rounded-full border-sky-400/30 bg-sky-500/10 px-3 py-1.5 text-xs text-sky-100 hover:bg-sky-500/20"
                  >
                    <Sparkles className="mr-2 h-3.5 w-3.5" />
                    Refine flow
                  </Button>
                </div>
              ) : null}
              <div className="min-h-0 flex-1 overflow-hidden rounded-[1.75rem] border border-slate-700/70" style={{ backgroundColor: "#111827" }}>
                {!hasSelectedSession ? (
                  <div className="flex h-full items-center justify-center">
                    <div className="flex max-w-md flex-col items-center rounded-2xl border border-slate-700/70 bg-[#182235] px-6 py-5 text-center">
                      <p className="text-sm font-medium text-slate-200">No task selected</p>
                      <p className="mt-2 text-xs leading-5 text-slate-400">
                        Create a task to start an execution tree. Orchestration tools appear after a task exists.
                      </p>
                      <Button
                        onClick={onCreateSession}
                        className="mt-4 rounded-2xl bg-sky-500 text-slate-950 hover:bg-sky-400"
                      >
                        <Sparkles className="mr-2 h-4 w-4" />
                        New task
                      </Button>
                    </div>
                  </div>
                ) : (
                  <DecisionCanvas
                    treeNodes={treeNodes}
                    allNodes={treeNodes}
                    selectedNodeId={selectedNodeId}
                    onSelectNode={onSelectNode}
                    onForkNode={onForkNode}
                    onMergeNode={onMergeNode}
                    onCreateStructuralNode={onCreateStructuralNode}
                    flowMode={flowMode}
                    onRunNode={onRunNode}
                    onUpdateNode={onUpdateNode}
                    onUpdateNodeAgent={onUpdateNodeAgent}
                    onDeleteNode={onDeleteNode}
                    onOpenNodeTerminal={onOpenNodeTerminal}
                    defaultExecutionAgent={defaultExecutionAgent}
                    orchestratorCurrentNodeId={orchestratorStatus?.current_node_id}
                    orchestratorActive={!!showOrchestratorActivity}
                    debugMode={debugMode}
                    onResetNode={onResetNode}
                  />
                )}
              </div>
            </>
          ) : (
            <div className="flex flex-1 items-center justify-center rounded-[1.75rem] border border-slate-700/70 bg-[#121a2a]">
              <div className="text-center">
                <p className="text-sm text-slate-300">
                  Choose a project folder from the sidebar
                </p>
                <p className="text-xs text-slate-400 mt-1">
                  Start new or open existing code to begin
                </p>
              </div>
            </div>
          )}
        </main>

        <ResizeHandle onMouseDown={inspector.onMouseDown} />

        {/* Inspector / Activity panel */}
        <aside style={{ width: inspector.size, minWidth: inspector.size }} className="shrink-0 min-h-0 overflow-hidden">
          {showOrchestratorActivity && selectedProject ? (
            <OrchestratorActivity
              agentType={selectedProject.agent_type}
              treeNodes={treeNodes}
              orchestratorStatus={orchestratorStatus}
              onSelectNode={(id) => onSelectNode(id)}
              onValidateRuntime={onValidateRuntime}
              onSendEnter={onSendEnterToNode}
              onResumeNode={onResumeNode}
              onStopNode={onStopNode}
            />
          ) : selectedNode && selectedProject ? (
            <InspectorPanel
              node={selectedNode}
              project={selectedProject}
              allNodes={treeNodes}
              flowMode={flowMode}
              onClose={onCloseTerminal}
              onFork={onForkNode}
              onMerge={onMergeNode}
              onCreateStructuralNode={onCreateStructuralNode}
              onPause={onPauseNode}
              onResume={onResumeNode}
              onDelete={onDeleteNode}
              onRunNode={onRunNode}
              onUpdateNode={onUpdateNode}
              onUpdateNodeAgent={onUpdateNodeAgent}
              onValidateRuntime={onValidateRuntime}
              onSendEnter={onSendEnterToNode}
              onStop={onStopNode}
              onRetryNode={onRetryNode}
              onResetNode={onResetNode}
              onOpenTerminal={onOpenNodeTerminal}
              defaultExecutionAgent={defaultExecutionAgent}
              manualTerminalSessionId={manualTerminalSessionId}
            />
          ) : debugMode && treeNodes.length > 0 ? (
            <ToonViewer nodes={treeNodes} />
          ) : (
            <div className="flex h-full items-center justify-center rounded-[1.75rem] border border-slate-700/70 bg-[#121a2a]">
              <div className="text-center px-6">
                <p className="text-sm text-slate-300">No node selected</p>
                <p className="text-xs text-slate-400 mt-1">
                  Click a node in the graph to inspect it
                </p>
              </div>
            </div>
          )}
        </aside>
      </div>

      {/* Merge dialog */}
      {terminalNode && selectedSessionId && (
        <MergeDialog
          open={mergeDialogOpen}
          onOpenChange={setMergeDialogOpen}
          terminalNode={terminalNode}
          sessionRootId={selectedSessionId}
          currentBranch={currentBranch ?? "main"}
          onComplete={onMergeComplete ?? (() => {})}
        />
      )}
    </div>
  );
}

// ─── TOON Context Viewer (debug mode) ────────────────────────

function ToonViewer({ nodes }: { nodes: DecisionNode[] }) {
  const [selectedId, setSelectedId] = useState<string>(nodes[0]?.id ?? "");
  const [toonContent, setToonContent] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Filter to runnable nodes only (skip decision nodes)
  const runnableNodes = useMemo(
    () => nodes.filter((n) => n.node_type !== "decision"),
    [nodes],
  );

  useEffect(() => {
    if (!selectedId) return;
    setLoading(true);
    setError(null);
    getNodeContext(selectedId)
      .then(setToonContent)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [selectedId]);

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-[1.75rem] border border-slate-700/70 bg-[#121a2a]">
      {/* Header */}
      <div className="flex items-center gap-3 border-b border-slate-700/70 px-4 py-3">
        <FileCode className="h-4 w-4 text-amber-400" />
        <span className="text-[11px] uppercase tracking-[0.22em] text-slate-400">
          TOON Context
        </span>
        <select
          value={selectedId}
          onChange={(e) => setSelectedId(e.target.value)}
          className="ml-auto rounded-lg border border-slate-600/70 bg-[#0f1726] px-2 py-1 text-xs text-slate-100 outline-none focus:border-sky-400/60"
        >
          {runnableNodes.map((n) => (
            <option key={n.id} value={n.id}>
              {n.label} ({n.node_type ?? "agent"})
            </option>
          ))}
        </select>
      </div>
      {/* Content */}
      <div className="flex-1 min-h-0 overflow-auto p-4">
        {loading ? (
          <div className="flex items-center gap-2 text-xs text-slate-400">
            <Loader2 className="h-3 w-3 animate-spin" />
            Loading context...
          </div>
        ) : error ? (
          <div className="text-xs text-rose-400">{error}</div>
        ) : (
          <pre className="whitespace-pre-wrap break-words text-[11px] leading-relaxed text-slate-300 font-mono">
            {toonContent}
          </pre>
        )}
      </div>
    </div>
  );
}
