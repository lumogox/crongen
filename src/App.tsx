import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentProviderReadiness,
  AgentRole,
  AgentType,
  AppSettings,
  DecisionNode,
  ModalType,
  NodeStatus,
  OrchestratorMode,
  OrchestratorStatus,
  PendingDecision,
  Project,
} from "./types";
import {
  getAgentProviderStatuses,
  getProjects,
  createProject,
  updateProject,
  deleteProject,
  getDecisionTree,
  runProjectNow,
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
import { getAgentLabel } from "./lib/agent-templates";
import { ContentArea } from "./components/ContentArea";
import { ProjectModal } from "./components/ProjectModal";
import { DeleteConfirm } from "./components/DeleteConfirm";
import { ForkModal } from "./components/ForkModal";
import { SessionModal } from "./components/SessionModal";
import { DeleteNodeConfirm } from "./components/DeleteNodeConfirm";
import { OrchestratorDecisionModal } from "./components/OrchestratorDecisionModal";
import { SettingsModal } from "./components/SettingsModal";

const DEFAULT_SETTINGS: AppSettings = {
  debug_mode: false,
  agent_setup_seen: false,
  planning_agent: null,
  execution_agent: null,
  planning_model: null,
  execution_model: null,
};

function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
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
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [agentProviderStatuses, setAgentProviderStatuses] = useState<AgentProviderReadiness[]>([]);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [agentStatusesLoaded, setAgentStatusesLoaded] = useState(false);
  const selectedProjectIdRef = useRef<string | null>(null);
  const selectedSessionIdRef = useRef<string | null>(null);

  // Load projects + settings on mount
  useEffect(() => {
    loadProjects();
    getSettings()
      .then((loaded) => setSettings({ ...DEFAULT_SETTINGS, ...loaded }))
      .catch(() => {})
      .finally(() => setSettingsLoaded(true));
    refreshAgentProviderStatuses().finally(() => setAgentStatusesLoaded(true));
  }, []);

  useEffect(() => {
    selectedProjectIdRef.current = selectedProjectId;
  }, [selectedProjectId]);

  useEffect(() => {
    selectedSessionIdRef.current = selectedSessionId;
  }, [selectedSessionId]);

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

  // Load sessions (root nodes) when selected project changes
  useEffect(() => {
    if (!selectedProjectId) {
      setSessions([]);
      setSelectedSessionId(null);
      return;
    }
    getRootNodes(selectedProjectId)
      .then(setSessions)
      .catch((e) => setError(String(e)));
  }, [selectedProjectId]);

  // Fetch current branch for the selected project's repo
  useEffect(() => {
    if (!selectedProjectId) { setCurrentBranch(null); return; }
    getRepoBranch(selectedProjectId)
      .then(setCurrentBranch)
      .catch(() => setCurrentBranch(null));
  }, [selectedProjectId]);

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
    if (!selectedProjectId) {
      setTreeNodes([]);
      setSelectedNodeId(null);
      setOrchestratorStatus(null);
      return;
    }
    setTreeLoading(true);
    setSelectedNodeId(selectedSessionId);
    setOrchestratorStatus(null);
    if (!selectedSessionId) {
      // No session selected — show empty canvas
      setTreeNodes([]);
      setTreeLoading(false);
      return;
    }
    getDecisionTree(selectedProjectId)
      .then((nodes) => {
        setTreeNodes(filterSessionSubtree(nodes, selectedSessionId));
        // Restore orchestrator status if this session has an active run
        getOrchestratorStatus(selectedSessionId)
          .then((status) => { if (status) setOrchestratorStatus(status); })
          .catch(() => {});
      })
      .catch((e) => setError(String(e)))
      .finally(() => setTreeLoading(false));
  }, [selectedProjectId, selectedSessionId]);

  // ─── Tauri event listeners ─────────────────────────────────
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    listen<{ session_id: string; node_id: string; project_id: string }>("session_started", (event) => {
      setTreeNodes((prev) =>
        prev.map((n) =>
          n.id === event.payload.node_id
            ? { ...n, status: "running" as NodeStatus, updated_at: Math.floor(Date.now() / 1000) }
            : n,
        ),
      );

      const activeProjectId = selectedProjectIdRef.current;
      if (activeProjectId && event.payload.project_id === activeProjectId) {
        getRootNodes(activeProjectId)
          .then((roots) => {
            if (selectedProjectIdRef.current === activeProjectId) {
              setSessions(roots);
            }
          })
          .catch(() => {});
      }
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
      setSessions((prev) =>
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
      setSessions((prev) =>
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
      setSessions((prev) =>
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
        const activeProjectId = selectedProjectIdRef.current;
        const activeSessionId = selectedSessionIdRef.current;
        if (activeProjectId && activeSessionId === event.payload.session_id) {
          getDecisionTree(activeProjectId)
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

  async function loadProjects() {
    try {
      const list = await getProjects();
      setProjects(list);
    } catch (e) {
      setError(String(e));
    }
  }

  async function refreshAgentProviderStatuses() {
    try {
      const statuses = await getAgentProviderStatuses();
      setAgentProviderStatuses(statuses);
    } catch (e) {
      setError(String(e));
    }
  }

  const selectedProject =
    projects.find((project) => project.id === selectedProjectId) ?? null;

  const providerStatusByType = useMemo(
    () => new Map(agentProviderStatuses.map((status) => [status.agent_type, status])),
    [agentProviderStatuses],
  );

  const getProviderStatus = useCallback(
    (agentType: AgentType | null | undefined) => {
      if (!agentType) return null;
      return providerStatusByType.get(agentType) ?? null;
    },
    [providerStatusByType],
  );

  const planningProviderStatus = getProviderStatus(settings.planning_agent);
  const executionDefaultStatus = getProviderStatus(settings.execution_agent);
  const selectedProjectExecutionStatus = useMemo<AgentProviderReadiness | null>(() => {
    if (!selectedProject) {
      return null;
    }
    if (selectedProject.agent_type === "custom") {
      return {
        agent_type: "custom",
        status: "ready",
        detail: "Custom execution is configured per project and does not use Agent Bay validation.",
        ready: true,
        supports_planning: false,
        supports_execution: true,
        coming_soon: false,
      };
    }
    return getProviderStatus(selectedProject.agent_type);
  }, [getProviderStatus, selectedProject]);

  const hasReadyPlanningDefault = !!settings.planning_agent && !!planningProviderStatus?.ready;
  const hasReadyExecutionDefault = !!settings.execution_agent && !!executionDefaultStatus?.ready;

  const setupReminder = useMemo(() => {
    if (!agentStatusesLoaded) return null;

    const issues: string[] = [];
    if (!settings.planning_agent) {
      issues.push("Choose a planning agent");
    } else if (!planningProviderStatus?.ready) {
      issues.push(`${getAgentLabel(settings.planning_agent)} is not ready for planning`);
    }

    if (!settings.execution_agent) {
      issues.push("Choose a default execution agent");
    } else if (!executionDefaultStatus?.ready) {
      issues.push(`${getAgentLabel(settings.execution_agent)} is not ready by default`);
    }

    if (selectedProject && selectedProjectExecutionStatus && !selectedProjectExecutionStatus.ready) {
      issues.push(`${selectedProject.name} cannot run because ${getAgentLabel(selectedProject.agent_type)} is not ready`);
    }

    if (issues.length === 0) return null;
    return issues[0];
  }, [
    agentStatusesLoaded,
    executionDefaultStatus,
    planningProviderStatus,
    selectedProject,
    selectedProjectExecutionStatus,
    settings.execution_agent,
    settings.planning_agent,
  ]);

  const getPrimaryMissingRole = useCallback((): AgentRole => {
    if (!settings.planning_agent || !planningProviderStatus?.ready) {
      return "planning";
    }
    return "execution";
  }, [planningProviderStatus?.ready, settings.planning_agent]);

  const openAgentBay = useCallback((options?: { forceSetup?: boolean; onboarding?: boolean; focusRole?: AgentRole }) => {
    setModal({
      kind: "settings",
      forceSetup: options?.forceSetup,
      onboarding: options?.onboarding,
      focusRole: options?.focusRole,
    });
  }, []);

  const guardAgentRole = useCallback(
    (
      role: AgentRole,
      agentType: AgentType | null | undefined,
      fallbackLabel: string,
    ) => {
      if (role === "execution" && agentType === "custom") {
        return true;
      }

      const status = getProviderStatus(agentType);
      if (agentType && status?.ready) {
        return true;
      }

      const label = getAgentLabel(agentType) || fallbackLabel;
      if (!agentType) {
        setError(`Choose a ${role} agent in Agent Bay before continuing.`);
      } else if (status?.status === "missing_cli") {
        setError(`${label} is not installed yet. Finish setup in Agent Bay.`);
      } else if (status?.status === "needs_login") {
        setError(`${label} still needs login. Finish setup in Agent Bay.`);
      } else if (status?.status === "coming_soon") {
        setError(`${label} support is coming soon. Choose Claude Code or Codex in Agent Bay.`);
      } else {
        setError(`${label} is not ready yet. Open Agent Bay to continue.`);
      }

      openAgentBay({ focusRole: role });
      return false;
    },
    [getProviderStatus, openAgentBay],
  );

  useEffect(() => {
    if (!settingsLoaded || !agentStatusesLoaded) return;
    if (settings.agent_setup_seen) return;
    if (hasReadyPlanningDefault && hasReadyExecutionDefault) return;
    if (modal) return;

    openAgentBay({
      forceSetup: true,
      onboarding: true,
      focusRole: getPrimaryMissingRole(),
    });
  }, [
    agentStatusesLoaded,
    getPrimaryMissingRole,
    hasReadyExecutionDefault,
    hasReadyPlanningDefault,
    modal,
    openAgentBay,
    settings.agent_setup_seen,
    settingsLoaded,
  ]);

  // ─── Project handlers ──────────────────────────────────────

  const handleNewProject = useCallback(() => {
    setModal({ kind: "create_project" });
  }, []);

  const handleEditProject = useCallback((project: Project) => {
    setModal({ kind: "edit_project", project });
  }, []);

  const handleDeleteProject = useCallback((project: Project) => {
    setModal({ kind: "delete_project", project });
  }, []);

  const handleSaveProject = useCallback(
    async (params: Parameters<typeof createProject>[0] & { id?: string; isActive: boolean; projectMode?: string }) => {
      try {
        if (params.id) {
          await updateProject({
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
          const created = await createProject({
            name: params.name,
            prompt: params.prompt,
            repoPath: params.repoPath,
            agentType: params.agentType,
            typeConfig: params.typeConfig,
            projectMode: params.projectMode,
          });
          setSelectedProjectId(created.id);
        }
        await loadProjects();
        setModal(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (modal?.kind !== "delete_project") return;
    try {
      await deleteProject(modal.project.id);
      if (selectedProjectId === modal.project.id) {
        setSelectedProjectId(null);
      }
      await loadProjects();
      setModal(null);
    } catch (e) {
      setError(String(e));
    }
  }, [modal, selectedProjectId]);

  // ─── Tree handlers ─────────────────────────────────────────

  const handleRunNow = useCallback(
    async (projectId: string) => {
      const project = projects.find((entry) => entry.id === projectId) ?? null;
      if (!project || !guardAgentRole("execution", project.agent_type, "Execution")) return;
      try {
        const rootNode = await runProjectNow(projectId);
        setSessions((prev) => [rootNode, ...prev.filter((session) => session.id !== rootNode.id)]);
        setSelectedSessionId(rootNode.id);
        setTreeNodes([rootNode]);
        setSelectedNodeId(rootNode.id);
      } catch (e) {
        setError(String(e));
      }
    },
    [guardAgentRole, projects],
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
      if (modal?.kind !== "create_structural_node" || !selectedProjectId) return;
      try {
        const newNode = await createStructuralNode({
          projectId: selectedProjectId,
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
    [modal, selectedProjectId],
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
          if (selectedProjectId) {
            const updated = await getDecisionTree(selectedProjectId);
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
    [selectedProjectId, selectedSessionId],
  );

  // ─── Flow drawing handlers ────────────────────────────────

  const handleCreateSession = useCallback(
    async (label: string, prompt: string) => {
      if (!selectedProjectId) return;
      try {
        const rootNode = await createRootNode(selectedProjectId, label, prompt);
        setSessions((prev) => [rootNode, ...prev]);
        setTreeNodes((prev) => [...prev, rootNode]);
        setSelectedSessionId(rootNode.id);
        setSelectedNodeId(rootNode.id);
        setModal(null);
      } catch (e) {
        setError(String(e));
      }
    },
    [selectedProjectId],
  );

  const handleRunNode = useCallback(
    async (nodeId: string) => {
      if (!selectedProject || !guardAgentRole("execution", selectedProject.agent_type, "Execution")) {
        return;
      }
      try {
        const updated = await runNode(nodeId);
        setTreeNodes((prev) =>
          prev.map((n) => (n.id === updated.id ? updated : n)),
        );
      } catch (e) {
        setError(String(e));
      }
    },
    [guardAgentRole, selectedProject],
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
      if (!selectedProject || !guardAgentRole("execution", selectedProject.agent_type, "Execution")) {
        return;
      }
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
    [guardAgentRole, selectedProject, selectedSessionId, treeNodes],
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
      if (!selectedProjectId || !selectedProject) return;
      if (!guardAgentRole("execution", selectedProject.agent_type, "Execution")) return;
      let rootNode: DecisionNode | null = null;
      try {
        setIsGeneratingPlan(true);
        // Derive a short label: take first sentence or first N words
        const firstSentence = prompt.split(/[.\n]/)[0].trim();
        const label = (firstSentence.length <= 50
          ? firstSentence
          : firstSentence.split(/\s+/).slice(0, 6).join(" ")
        ) || "Quick task";
        rootNode = await createRootNode(selectedProjectId, label, prompt);
        // Auto-run the node
        const updated = await runNode(rootNode.id);
        setSessions((prev) => [updated, ...prev.filter((session) => session.id !== updated.id)]);
        setSelectedSessionId(updated.id);
        setTreeNodes([updated]);
        setSelectedNodeId(updated.id);
        setModal(null);
      } catch (e) {
        if (rootNode) {
          try {
            await deleteNodeBranch(rootNode.id);
          } catch (cleanupError) {
            console.warn("Failed to clean up quick-run draft node", cleanupError);
          }
        }
        throw e;
      } finally {
        setIsGeneratingPlan(false);
      }
    },
    [guardAgentRole, selectedProject, selectedProjectId],
  );

  const handleGeneratePlan = useCallback(
    async (prompt: string, complexity?: "linear" | "branching") => {
      if (!selectedProjectId) return;
      if (!guardAgentRole("planning", settings.planning_agent, "Planning")) return;
      try {
        setIsGeneratingPlan(true);
        const nodes = await generatePlan(selectedProjectId, prompt, complexity);
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
    [guardAgentRole, selectedProjectId, settings.planning_agent],
  );

  // ─── Settings handlers ─────────────────────────────────────

  const handleSaveSettings = useCallback(async (newSettings: AppSettings) => {
    try {
      const merged = { ...DEFAULT_SETTINGS, ...newSettings };
      await updateSettings(merged);
      setSettings(merged);
      await refreshAgentProviderStatuses();
    } catch (e) {
      setError(String(e));
      throw e;
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
    if (selectedProjectId) {
      const updated = await getDecisionTree(selectedProjectId);
      setTreeNodes(filterSessionSubtree(updated, selectedSessionId));
    }
  }, [selectedProjectId, selectedSessionId]);

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
            projects={projects}
            selectedProjectId={selectedProjectId}
            onSelectProject={setSelectedProjectId}
            onNewProject={handleNewProject}
            onEditProject={handleEditProject}
            onDeleteProject={handleDeleteProject}
            selectedProject={selectedProject}
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
            agentSetupReminder={setupReminder}
            onOpenSettings={() => openAgentBay({ focusRole: getPrimaryMissingRole() })}
            onResetNode={handleResetNode}
          />
        </div>
      </div>

      {/* Modals */}
      {modal?.kind === "create_project" && (
        <ProjectModal
          mode="create"
          defaultExecutionAgent={settings.execution_agent}
          onSave={handleSaveProject}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "edit_project" && (
        <ProjectModal
          mode="edit"
          project={modal.project}
          defaultExecutionAgent={settings.execution_agent}
          onSave={handleSaveProject}
          onClose={() => setModal(null)}
        />
      )}
      {modal?.kind === "delete_project" && (
        <DeleteConfirm
          project={modal.project}
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
          planningAgentLabel={getAgentLabel(settings.planning_agent)}
          executionAgentLabel={selectedProject ? getAgentLabel(selectedProject.agent_type) : getAgentLabel(settings.execution_agent)}
          planningStatus={planningProviderStatus}
          executionStatus={selectedProjectExecutionStatus}
          onOpenAgentSetup={(role) => openAgentBay({ focusRole: role })}
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
          statuses={agentProviderStatuses}
          forceSetup={modal.forceSetup}
          onboarding={modal.onboarding}
          focusRole={modal.focusRole}
          onSave={handleSaveSettings}
          onRefreshStatuses={refreshAgentProviderStatuses}
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
