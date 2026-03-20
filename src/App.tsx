import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Agent, AppSettings, DecisionNode, ModalType, NodeStatus, OrchestratorMode, OrchestratorStatus, PendingDecision } from "./types";
import {
  getAgents,
  createAgent,
  updateAgent,
  deleteAgent,
  getDecisionTree,
  runAgentNow,
  forkNode,
  createStructuralNode,
  mergeNodeBranch,
  deleteNodeBranch,
  pauseSession,
  resumeSession,
  createRootNode,
  runNode,
  updateNode,
  getRootNodes,
  startOrchestrator,
  getOrchestratorStatus,
  submitOrchestratorDecision,
  cancelOrchestrator,
  generatePlan,
  getSettings,
  updateSettings,
  resetNodeStatus,
  getRepoBranch,
} from "./lib/tauri-commands";
import { ContentArea } from "./components/ContentArea";
import { AgentModal } from "./components/AgentModal";
import { DeleteConfirm } from "./components/DeleteConfirm";
import { ForkModal } from "./components/ForkModal";
import { SessionModal } from "./components/SessionModal";
import { DeleteNodeConfirm } from "./components/DeleteNodeConfirm";
import { OrchestratorDecisionModal } from "./components/OrchestratorDecisionModal";
import { SettingsModal } from "./components/SettingsModal";

function App() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [modal, setModal] = useState<ModalType>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  // ─── Tree state ────────────────────────────────────────────
  const [treeNodes, setTreeNodes] = useState<DecisionNode[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [treeLoading, setTreeLoading] = useState(false);

  // ─── Flow drawing state ──────────────────────────────────
  const [flowMode, setFlowMode] = useState<"linear" | "branching">("branching");
  const [sessions, setSessions] = useState<DecisionNode[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  // ─── Orchestrator state ──────────────────────────────────
  const [orchestratorStatus, setOrchestratorStatus] = useState<OrchestratorStatus | null>(null);
  const [isGeneratingPlan, setIsGeneratingPlan] = useState(false);

  // ─── Current branch ────────────────────────────────────────
  const [currentBranch, setCurrentBranch] = useState<string | null>(null);

  // ─── Settings state ────────────────────────────────────────
  const [settings, setSettings] = useState<AppSettings>({ debug_mode: false });

  // Load agents + settings on mount
  useEffect(() => {
    loadAgents();
    getSettings().then(setSettings).catch(() => {});
  }, []);

  // Clear error after 5s
  useEffect(() => {
    if (!error) return;
    const t = setTimeout(() => setError(null), 5000);
    return () => clearTimeout(t);
  }, [error]);

  // Clear success after 3s
  useEffect(() => {
    if (!success) return;
    const t = setTimeout(() => setSuccess(null), 3000);
    return () => clearTimeout(t);
  }, [success]);

  // Load sessions (root nodes) when selected agent changes
  useEffect(() => {
    if (!selectedAgentId) {
      setSessions([]);
      setSelectedSessionId(null);
      return;
    }
    getRootNodes(selectedAgentId)
      .then(setSessions)
      .catch((e) => setError(String(e)));
  }, [selectedAgentId]);

  // Fetch current branch for the selected agent's repo
  useEffect(() => {
    if (!selectedAgentId) { setCurrentBranch(null); return; }
    getRepoBranch(selectedAgentId)
      .then(setCurrentBranch)
      .catch(() => setCurrentBranch(null));
  }, [selectedAgentId]);

  // Filter nodes to a session's subtree
  function filterSessionSubtree(nodes: DecisionNode[], sessionId: string | null): DecisionNode[] {
    if (!sessionId) return [];
    const ids = new Set<string>();
    const queue = [sessionId];
    while (queue.length > 0) {
      const id = queue.shift()!;
      ids.add(id);
      for (const n of nodes) {
        if (n.parent_id === id && !ids.has(n.id)) queue.push(n.id);
      }
    }
    return nodes.filter((n) => ids.has(n.id));
  }

  // Load tree when selected session changes
  useEffect(() => {
    if (!selectedAgentId) {
      setTreeNodes([]);
      setSelectedNodeId(null);
      setOrchestratorStatus(null);
      return;
    }
    setTreeLoading(true);
    setSelectedNodeId(null);
    setOrchestratorStatus(null);
    if (!selectedSessionId) {
      // No session selected — show empty canvas
      setTreeNodes([]);
      setTreeLoading(false);
      return;
    }
    getDecisionTree(selectedAgentId)
      .then((nodes) => {
        setTreeNodes(filterSessionSubtree(nodes, selectedSessionId));
        // Restore orchestrator status if this session has an active run
        getOrchestratorStatus(selectedSessionId)
          .then((status) => { if (status) setOrchestratorStatus(status); })
          .catch(() => {});
      })
      .catch((e) => setError(String(e)))
      .finally(() => setTreeLoading(false));
  }, [selectedAgentId, selectedSessionId]);

  // ─── Tauri event listeners ─────────────────────────────────
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    listen<{ node_id: string }>("session_started", (event) => {
      setTreeNodes((prev) =>
        prev.map((n) =>
          n.id === event.payload.node_id
            ? { ...n, status: "running" as NodeStatus, updated_at: Math.floor(Date.now() / 1000) }
            : n,
        ),
      );
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<{ node_id: string; exit_code: number }>("session_ended", (event) => {
      const { node_id, exit_code } = event.payload;
      const newStatus: NodeStatus = exit_code === 0 ? "completed" : "failed";
      setTreeNodes((prev) =>
        prev.map((n) =>
          n.id === node_id
            ? {
                ...n,
                status: newStatus,
                exit_code,
                updated_at: Math.floor(Date.now() / 1000),
              }
            : n,
        ),
      );
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<{ node_id: string }>("session_paused", (event) => {
      setTreeNodes((prev) =>
        prev.map((n) =>
          n.id === event.payload.node_id
            ? { ...n, status: "paused" as NodeStatus, updated_at: Math.floor(Date.now() / 1000) }
            : n,
        ),
      );
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<{ node_id: string }>("session_resumed", (event) => {
      setTreeNodes((prev) =>
        prev.map((n) =>
          n.id === event.payload.node_id
            ? { ...n, status: "running" as NodeStatus, updated_at: Math.floor(Date.now() / 1000) }
            : n,
        ),
      );
    }).then((unlisten) => unlisteners.push(unlisten));

    // Orchestrator events
    listen<{ session_id: string; node_id: string; status: string; completed_count: number; total_count: number }>(
      "orchestrator_progress",
      (event) => {
        setOrchestratorStatus((prev) =>
          prev && prev.session_id === event.payload.session_id
            ? {
                ...prev,
                state: "running",
                current_node_id: event.payload.node_id,
                completed_count: event.payload.completed_count,
                total_count: event.payload.total_count,
              }
            : prev,
        );
      },
    ).then((unlisten) => unlisteners.push(unlisten));

    listen<{ session_id: string; decision: PendingDecision }>(
      "orchestrator_decision_needed",
      (event) => {
        setOrchestratorStatus((prev) =>
          prev && prev.session_id === event.payload.session_id
            ? { ...prev, state: "waiting_user", pending_decision: event.payload.decision }
            : prev,
        );
        setModal({
          kind: "orchestrator_decision",
          sessionId: event.payload.session_id,
          decision: event.payload.decision,
        });
      },
    ).then((unlisten) => unlisteners.push(unlisten));

    listen<{ session_id: string; success: boolean }>(
      "orchestrator_complete",
      (event) => {
        setOrchestratorStatus((prev) =>
          prev && prev.session_id === event.payload.session_id
            ? { ...prev, state: event.payload.success ? "complete" : "failed" }
            : prev,
        );
        // Reload tree to pick up all status changes (filter to current session)
        if (selectedAgentId) {
          getDecisionTree(selectedAgentId)
            .then((nodes) => {
              // Use event session_id as the filter key since it's fresh from the event
              const sid = event.payload.session_id;
              setTreeNodes(filterSessionSubtree(nodes, sid));
            })
            .catch(() => {});
        }
      },
    ).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  async function loadAgents() {
    try {
      const list = await getAgents();
      setAgents(list);
    } catch (e) {
      setError(String(e));
    }
  }

  const selectedAgent =
    agents.find((a) => a.id === selectedAgentId) ?? null;

  // ─── Agent handlers ────────────────────────────────────────

  const handleNewAgent = useCallback(() => {
    setModal({ kind: "create_agent" });
  }, []);

  const handleEditAgent = useCallback((agent: Agent) => {
    setModal({ kind: "edit_agent", agent });
  }, []);

  const handleDeleteAgent = useCallback((agent: Agent) => {
    setModal({ kind: "delete_agent", agent });
  }, []);

  const handleSaveAgent = useCallback(
    async (params: Parameters<typeof createAgent>[0] & { id?: string; isActive: boolean; projectMode?: string }) => {
      try {
        if (params.id) {
          await updateAgent({
            id: params.id,
            name: params.name,
            prompt: params.prompt,
            repoPath: params.repoPath,
            agentType: params.agentType,
            typeConfig: params.typeConfig,
            isActive: params.isActive,
            projectMode: params.projectMode,
          });
        } else {
          const created = await createAgent({
            name: params.name,
            prompt: params.prompt,
            repoPath: params.repoPath,
            agentType: params.agentType,
            typeConfig: params.typeConfig,
            projectMode: params.projectMode,
          });
          setSelectedAgentId(created.id);
        }
        await loadAgents();
        setModal(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (modal?.kind !== "delete_agent") return;
    try {
      await deleteAgent(modal.agent.id);
      if (selectedAgentId === modal.agent.id) {
        setSelectedAgentId(null);
      }
      await loadAgents();
      setModal(null);
    } catch (e) {
      setError(String(e));
    }
  }, [modal, selectedAgentId]);

  // ─── Tree handlers ─────────────────────────────────────────

  const handleRunNow = useCallback(
    async (agentId: string) => {
      try {
        const rootNode = await runAgentNow(agentId);
        setTreeNodes((prev) => [...prev, rootNode]);
        setSelectedNodeId(rootNode.id);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleSelectNode = useCallback((id: string | null) => {
    setSelectedNodeId(id);
  }, []);

  const handleForkNode = useCallback((nodeId: string) => {
    setModal({ kind: "fork_node", nodeId });
  }, []);

  const handleConfirmFork = useCallback(
    async (nodeId: string, label: string, prompt: string) => {
      try {
        const newNode = await forkNode(nodeId, label, prompt);
        setTreeNodes((prev) => [...prev, newNode]);
        setSelectedNodeId(newNode.id);
        setModal(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleOpenStructuralNodeModal = useCallback(
    (parentId: string | null, nodeType: "task" | "decision" | "agent" | "merge" | "final") => {
      setModal({ kind: "create_structural_node", parentId, nodeType });
    },
    [],
  );

  const handleConfirmStructuralNode = useCallback(
    async (_parentId: string, label: string, prompt: string) => {
      if (modal?.kind !== "create_structural_node" || !selectedAgentId) return;
      try {
        const newNode = await createStructuralNode({
          agentId: selectedAgentId,
          parentId: modal.parentId,
          label,
          prompt,
          nodeType: modal.nodeType,
        });
        setTreeNodes((prev) => [...prev, newNode]);
        setSelectedNodeId(newNode.id);
        setModal(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [modal, selectedAgentId],
  );

  const handlePauseNode = useCallback(
    async (nodeId: string) => {
      try {
        await pauseSession(nodeId);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleResumeNode = useCallback(
    async (nodeId: string) => {
      try {
        await resumeSession(nodeId);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleMergeNode = useCallback(
    async (nodeId: string) => {
      try {
        const result = await mergeNodeBranch(nodeId);
        if (result.success) {
          const msg = result.auto_resolved
            ? `Merged (auto-resolved ${result.conflict_files.length} conflict${result.conflict_files.length === 1 ? "" : "s"})`
            : "Branch merged into main successfully";
          setSuccess(msg);
          if (selectedAgentId) {
            const updated = await getDecisionTree(selectedAgentId);
            setTreeNodes(filterSessionSubtree(updated, selectedSessionId));
          }
        } else {
          setError(
            `Merge conflict in: ${result.conflict_files.join(", ")}`,
          );
        }
      } catch (e) {
        setError(String(e));
      }
    },
    [selectedAgentId, selectedSessionId],
  );

  // ─── Flow drawing handlers ────────────────────────────────

  const handleCreateSession = useCallback(
    async (label: string, prompt: string) => {
      if (!selectedAgentId) return;
      try {
        const rootNode = await createRootNode(selectedAgentId, label, prompt);
        setSessions((prev) => [rootNode, ...prev]);
        setTreeNodes((prev) => [...prev, rootNode]);
        setSelectedSessionId(rootNode.id);
        setSelectedNodeId(rootNode.id);
        setModal(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [selectedAgentId],
  );

  const handleRunNode = useCallback(
    async (nodeId: string) => {
      try {
        const updated = await runNode(nodeId);
        setTreeNodes((prev) =>
          prev.map((n) => (n.id === updated.id ? updated : n)),
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleUpdateNode = useCallback(
    async (nodeId: string, label: string, prompt: string) => {
      try {
        const updated = await updateNode(nodeId, label, prompt);
        setTreeNodes((prev) =>
          prev.map((n) => (n.id === updated.id ? updated : n)),
        );
        // Also update sessions list if it's a root node
        setSessions((prev) =>
          prev.map((n) => (n.id === updated.id ? updated : n)),
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  // ─── Delete node handlers ──────────────────────────────────

  const handleDeleteNode = useCallback(
    (nodeId: string) => {
      const node = treeNodes.find((n) => n.id === nodeId);
      if (node) setModal({ kind: "delete_node", node });
    },
    [treeNodes],
  );

  const handleConfirmDeleteNode = useCallback(async () => {
    if (modal?.kind !== "delete_node") return;
    try {
      const deletedIds = await deleteNodeBranch(modal.node.id);
      const deletedSet = new Set(deletedIds);
      setTreeNodes((prev) => prev.filter((n) => !deletedSet.has(n.id)));
      setSelectedNodeId((prev) => (prev && deletedSet.has(prev) ? null : prev));
      setSuccess("Node and branch deleted successfully");
      setModal(null);
    } catch (e) {
      setError(String(e));
    }
  }, [modal]);

  // ─── Orchestrator handlers ────────────────────────────────

  const handleStartOrchestrator = useCallback(
    async (mode: OrchestratorMode) => {
      if (!selectedSessionId) return;
      try {
        // Set initial status BEFORE starting — ensures event handlers have a
        // non-null `prev` so no progress events are lost during startup.
        const runnableCount = treeNodes.filter((n) => n.node_type !== "decision").length;
        setOrchestratorStatus({
          session_id: selectedSessionId,
          state: "running",
          mode,
          current_node_id: null,
          completed_count: 0,
          total_count: runnableCount,
          pending_decision: null,
        });
        await startOrchestrator(selectedSessionId, mode);
        // Fetch authoritative status from backend (has correct already_completed
        // count and total_count computed server-side)
        const status = await getOrchestratorStatus(selectedSessionId);
        if (status) {
          setOrchestratorStatus(status);
        }
      } catch (e) {
        setError(String(e));
        setOrchestratorStatus(null);
      }
    },
    [selectedSessionId, treeNodes],
  );

  const handleSubmitDecision = useCallback(
    async (selectedNodeId: string) => {
      if (!orchestratorStatus) return;
      try {
        await submitOrchestratorDecision(orchestratorStatus.session_id, selectedNodeId);
        setModal(null);
        setOrchestratorStatus((prev) =>
          prev ? { ...prev, state: "running", pending_decision: null } : null,
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [orchestratorStatus],
  );

  const handleCancelOrchestrator = useCallback(async () => {
    if (!orchestratorStatus) return;
    try {
      await cancelOrchestrator(orchestratorStatus.session_id);
      setOrchestratorStatus(null);
    } catch (e) {
      setError(String(e));
    }
  }, [orchestratorStatus]);

  const handleQuickRun = useCallback(
    async (prompt: string) => {
      if (!selectedAgentId) return;
      try {
        setIsGeneratingPlan(true);
        // Derive a short label: take first sentence or first N words
        const firstSentence = prompt.split(/[.\n]/)[0].trim();
        const label = (firstSentence.length <= 50
          ? firstSentence
          : firstSentence.split(/\s+/).slice(0, 6).join(" ")
        ) || "Quick task";
        const rootNode = await createRootNode(selectedAgentId, label, prompt);
        setSessions((prev) => [rootNode, ...prev]);
        setSelectedSessionId(rootNode.id);
        setTreeNodes([rootNode]);
        setSelectedNodeId(rootNode.id);
        setModal(null);
        // Auto-run the node
        const updated = await runNode(rootNode.id);
        setTreeNodes([updated]);
      } catch (e) {
        throw e;
      } finally {
        setIsGeneratingPlan(false);
      }
    },
    [selectedAgentId],
  );

  const handleGeneratePlan = useCallback(
    async (prompt: string, complexity?: "linear" | "branching") => {
      if (!selectedAgentId) return;
      try {
        setIsGeneratingPlan(true);
        const nodes = await generatePlan(selectedAgentId, prompt, complexity);
        // Add nodes to tree and select the root session
        setTreeNodes((prev) => [...prev, ...nodes]);
        if (nodes.length > 0) {
          const rootNode = nodes[0];
          setSessions((prev) => [rootNode, ...prev]);
          setSelectedSessionId(rootNode.id);
          setSelectedNodeId(rootNode.id);
        }
        setModal(null);
        setSuccess(`Generated plan with ${nodes.length} nodes`);
      } catch (e) {
        // Re-throw so SessionModal can display the error inline
        throw e;
      } finally {
        setIsGeneratingPlan(false);
      }
    },
    [selectedAgentId],
  );

  // ─── Settings handlers ─────────────────────────────────────

  const handleSaveSettings = useCallback(async (newSettings: AppSettings) => {
    try {
      await updateSettings(newSettings);
      setSettings(newSettings);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const handleResetNode = useCallback(async (nodeId: string) => {
    try {
      const updated = await resetNodeStatus(nodeId);
      setTreeNodes((prev) =>
        prev.map((n) => (n.id === updated.id ? updated : n)),
      );
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const handleMergeComplete = useCallback(async () => {
    if (selectedSessionId) {
      setSessions((prev) =>
        prev.map((s) => s.id === selectedSessionId ? { ...s, status: "merged" as const } : s),
      );
    }
    if (selectedAgentId) {
      const updated = await getDecisionTree(selectedAgentId);
      setTreeNodes(filterSessionSubtree(updated, selectedSessionId));
    }
  }, [selectedAgentId, selectedSessionId]);

  // Find parent node for fork / structural modals
  const forkParentNode =
    modal?.kind === "fork_node"
      ? treeNodes.find((n) => n.id === modal.nodeId) ?? null
      : null;

  const structuralParentNode =
    modal?.kind === "create_structural_node" && modal.parentId
      ? treeNodes.find((n) => n.id === modal.parentId) ?? null
      : null;

  return (
    <div className="min-h-screen bg-[#020617] text-slate-100">
      <div className="h-screen overflow-hidden p-4">
        <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-[2rem] border border-white/10 bg-slate-950/70 p-4 shadow-[0_30px_120px_rgba(2,6,23,0.65)] backdrop-blur-xl">
          <ContentArea
            agents={agents}
            selectedAgentId={selectedAgentId}
            onSelectAgent={setSelectedAgentId}
            onNewAgent={handleNewAgent}
            onEditAgent={handleEditAgent}
            onDeleteAgent={handleDeleteAgent}
            selectedAgent={selectedAgent}
            treeNodes={treeNodes}
            treeLoading={treeLoading}
            selectedNodeId={selectedNodeId}
            onSelectNode={handleSelectNode}
            onForkNode={handleForkNode}
            onMergeNode={handleMergeNode}
            onCreateStructuralNode={handleOpenStructuralNodeModal}
            onRunNow={handleRunNow}
            onCloseTerminal={() => setSelectedNodeId(null)}
            onPauseNode={handlePauseNode}
            onResumeNode={handleResumeNode}
            onDeleteNode={handleDeleteNode}
            flowMode={flowMode}
            onFlowModeChange={setFlowMode}
            sessions={sessions}
            selectedSessionId={selectedSessionId}
            onSelectSession={setSelectedSessionId}
            onCreateSession={() => setModal({ kind: "create_session" })}
            onRunNode={handleRunNode}
            onUpdateNode={handleUpdateNode}
            orchestratorStatus={orchestratorStatus}
            onStartOrchestrator={handleStartOrchestrator}
            onCancelOrchestrator={handleCancelOrchestrator}
            onMergeComplete={handleMergeComplete}
            currentBranch={currentBranch}
            debugMode={settings.debug_mode}
            onOpenSettings={() => setModal({ kind: "settings" })}
            onResetNode={handleResetNode}
          />
        </div>
      </div>

      {/* Modals */}
      {modal?.kind === "create_agent" && (
        <AgentModal
          mode="create"
          onSave={handleSaveAgent}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "edit_agent" && (
        <AgentModal
          mode="edit"
          agent={modal.agent}
          onSave={handleSaveAgent}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "delete_agent" && (
        <DeleteConfirm
          agent={modal.agent}
          onConfirm={handleConfirmDelete}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "delete_node" && (
        <DeleteNodeConfirm
          node={modal.node}
          onConfirm={handleConfirmDeleteNode}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "create_session" && (
        <SessionModal
          onConfirm={handleCreateSession}
          onQuickRun={handleQuickRun}
          onGeneratePlan={handleGeneratePlan}
          isGenerating={isGeneratingPlan}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "orchestrator_decision" && (
        <OrchestratorDecisionModal
          decision={modal.decision}
          onSelect={handleSubmitDecision}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "settings" && (
        <SettingsModal
          settings={settings}
          onSave={handleSaveSettings}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "fork_node" && forkParentNode && (
        <ForkModal
          parentNode={forkParentNode}
          mode="fork"
          onConfirm={handleConfirmFork}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "create_structural_node" && structuralParentNode && (
        <ForkModal
          parentNode={structuralParentNode}
          mode={modal.nodeType}
          onConfirm={handleConfirmStructuralNode}
          onClose={() => setModal(null)}
        />
      )}

      {/* Success toast */}
      {success && (
        <div className="fixed bottom-10 right-4 z-50 max-w-sm rounded-2xl border border-emerald-400/20 bg-emerald-500/10 px-4 py-2 text-[12px] text-emerald-200 shadow-lg backdrop-blur">
          {success}
        </div>
      )}

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-10 right-4 z-50 max-w-sm rounded-2xl border border-rose-400/20 bg-rose-500/10 px-4 py-2 text-[12px] text-rose-200 shadow-lg backdrop-blur">
          {error}
        </div>
      )}
    </div>
  );
}

export default App;
