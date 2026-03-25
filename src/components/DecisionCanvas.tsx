import { useCallback, useEffect, useMemo } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  useReactFlow,
} from "@xyflow/react";
import type { DecisionNode } from "../types";
import { useTreeLayout } from "../hooks/useTreeLayout";
import { ExecutionNode } from "./ExecutionNode";
import { ExecutionEdge } from "./ExecutionEdge";

const nodeTypes = { executionNode: ExecutionNode };
const edgeTypes = { executionEdge: ExecutionEdge };

interface DecisionCanvasProps {
  treeNodes: DecisionNode[];
  allNodes: DecisionNode[];
  selectedNodeId: string | null;
  onSelectNode: (id: string | null) => void;
  onForkNode: (nodeId: string) => void;
  onMergeNode: (nodeId: string) => void;
  onCreateStructuralNode: (parentId: string | null, nodeType: "task" | "decision" | "agent" | "merge" | "final") => void;
  flowMode?: "linear" | "branching";
  onRunNode?: (nodeId: string) => void;
  onUpdateNode?: (nodeId: string, label: string, prompt: string) => void;
  onDeleteNode?: (nodeId: string) => void;
  onOpenNodeTerminal?: (nodeId: string) => void;
  orchestratorCurrentNodeId?: string | null;
  orchestratorActive?: boolean;
  debugMode?: boolean;
  onResetNode?: (nodeId: string) => void;
}

function getAncestryPath(
  nodes: DecisionNode[],
  selectedId: string | null,
): Set<string> {
  if (!selectedId) return new Set();
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));
  const path = new Set<string>();
  let current = selectedId;
  while (current) {
    path.add(current);
    const node = nodeMap.get(current);
    current = node?.parent_id ?? "";
  }
  return path;
}

export function DecisionCanvas({
  treeNodes,
  allNodes,
  selectedNodeId,
  onSelectNode,
  onForkNode,
  onMergeNode,
  onCreateStructuralNode,
  flowMode,
  onRunNode,
  onUpdateNode,
  onDeleteNode,
  onOpenNodeTerminal,
  orchestratorCurrentNodeId,
  orchestratorActive,
  debugMode,
  onResetNode,
}: DecisionCanvasProps) {
  const { fitView } = useReactFlow();
  const nodeSignature = useMemo(
    () => treeNodes.map((node) => node.id).join("|"),
    [treeNodes],
  );

  const ancestryPath = useMemo(
    () => getAncestryPath(allNodes, selectedNodeId),
    [allNodes, selectedNodeId],
  );

  const onFork = useCallback(
    (nodeId: string) => onForkNode(nodeId),
    [onForkNode],
  );
  const onMerge = useCallback(
    (nodeId: string) => onMergeNode(nodeId),
    [onMergeNode],
  );

  const onCreateStructural = useCallback(
    (parentId: string | null, nodeType: "task" | "decision" | "agent" | "merge" | "final") =>
      onCreateStructuralNode(parentId, nodeType),
    [onCreateStructuralNode],
  );

  const { flowNodes, flowEdges } = useTreeLayout({
    nodes: treeNodes,
    selectedNodeId,
    ancestryPath,
    onFork,
    onMerge,
    onCreateStructuralNode: onCreateStructural,
    flowMode: flowMode ?? "branching",
    onRunNode: onRunNode ?? (() => {}),
    onUpdateNode: onUpdateNode ?? (() => {}),
    onDeleteNode: onDeleteNode ?? (() => {}),
    onOpenNodeTerminal,
    orchestratorCurrentNodeId,
    debugMode,
    onResetNode,
  });

  // Fit view when tree changes or layout shifts (e.g. orchestrator panel appears/disappears)
  useEffect(() => {
    if (flowNodes.length > 0) {
      const t = setTimeout(() => fitView({ padding: 0.22, duration: 220 }), 80);
      return () => clearTimeout(t);
    }
  }, [fitView, nodeSignature, orchestratorActive, flowNodes.length]);

  // Escape to deselect
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onSelectNode(null);
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onSelectNode]);

  if (treeNodes.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex flex-col items-center gap-4 text-center">
          <div className="rounded-2xl border border-white/10 bg-white/[0.03] p-6">
            <div className="text-sm font-medium text-slate-300">
              No execution tree yet
            </div>
            <div className="mt-2 text-xs text-slate-500">
              Create a new task to start building an execution flow.
              <br />
              Then drag node types from the palette to design your tree.
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <ReactFlow
      nodes={flowNodes}
      edges={flowEdges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      fitView
      fitViewOptions={{ padding: 0.22 }}
      onNodeClick={(_e, node) => onSelectNode(node.id)}
      onPaneClick={() => onSelectNode(null)}
      onDragOver={(e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
      }}
      onDrop={(e) => {
        e.preventDefault();
        const type = e.dataTransfer.getData("application/crongen-node");
        if (!type) return;
        const parentId = selectedNodeId;
        if (type === "task" && treeNodes.length > 0) return;
        if (type === "task") return;
        if (parentId) {
          onCreateStructuralNode(parentId, type as "task" | "decision" | "agent" | "merge" | "final");
        }
      }}
      nodesDraggable
      nodesConnectable={false}
      elementsSelectable
      panOnDrag
      zoomOnScroll
      minZoom={0.5}
      maxZoom={1.4}
      proOptions={{ hideAttribution: true }}
      className="bg-transparent"
      defaultEdgeOptions={{ type: "smoothstep" }}
    >
      <Background
        color="rgba(148,163,184,0.14)"
        gap={28}
        size={1.2}
      />
      <Controls
        position="bottom-right"
        showInteractive={false}
      />
    </ReactFlow>
  );
}
