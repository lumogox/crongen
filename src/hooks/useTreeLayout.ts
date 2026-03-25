import { useMemo } from "react";
import dagre from "@dagrejs/dagre";
import { MarkerType, type Node, type Edge } from "@xyflow/react";
import type { DecisionNode, DecisionNodeData } from "../types";
import { inferNodeType, inferEdgeVariant } from "../lib/node-type-inference";

const NODE_WIDTH = 260;
const NODE_HEIGHT = 180;

interface UseTreeLayoutInput {
  nodes: DecisionNode[];
  selectedNodeId: string | null;
  ancestryPath: Set<string>;
  onFork: (nodeId: string) => void;
  onMerge: (nodeId: string) => void;
  onCreateStructuralNode: (parentId: string | null, nodeType: "task" | "decision" | "agent" | "merge" | "final") => void;
  flowMode?: "linear" | "branching";
  onRunNode?: (nodeId: string) => void;
  onUpdateNode?: (nodeId: string, label: string, prompt: string) => void;
  onDeleteNode?: (nodeId: string) => void;
  onOpenNodeTerminal?: (nodeId: string) => void;
  orchestratorCurrentNodeId?: string | null;
  debugMode?: boolean;
  onResetNode?: (nodeId: string) => void;
}

interface UseTreeLayoutOutput {
  flowNodes: Node<DecisionNodeData>[];
  flowEdges: Edge[];
}

export function useTreeLayout({
  nodes,
  selectedNodeId,
  ancestryPath,
  onFork,
  onMerge,
  onCreateStructuralNode,
  flowMode,
  onRunNode,
  onUpdateNode,
  onDeleteNode,
  onOpenNodeTerminal,
  orchestratorCurrentNodeId,
  debugMode,
  onResetNode,
}: UseTreeLayoutInput): UseTreeLayoutOutput {
  return useMemo(() => {
    if (nodes.length === 0) return { flowNodes: [], flowEdges: [] };

    // Build dagre graph
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: "TB", nodesep: 80, ranksep: 120 });
    g.setDefaultEdgeLabel(() => ({}));

    for (const node of nodes) {
      g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
    }

    for (const node of nodes) {
      if (node.parent_id) {
        g.setEdge(node.parent_id, node.id);
      }
    }

    // Add layout-only ordering edges so merge/final nodes rank below agent siblings.
    // Without these, Dagre places all children of a decision at the same rank.
    const childrenOf = new Map<string, DecisionNode[]>();
    for (const node of nodes) {
      if (node.parent_id) {
        const kids = childrenOf.get(node.parent_id) ?? [];
        kids.push(node);
        childrenOf.set(node.parent_id, kids);
      }
    }
    for (const [, siblings] of childrenOf) {
      const agents = siblings.filter((n) => n.node_type === "agent");
      const merge = siblings.find((n) => n.node_type === "merge");
      if (merge && agents.length > 0) {
        // Add edges from each agent to the merge → forces merge below agents
        for (const agent of agents) {
          g.setEdge(agent.id, merge.id, { weight: 0, minlen: 1 });
        }
      }
    }

    dagre.layout(g);

    // Map to React Flow nodes
    const flowNodes: Node<DecisionNodeData>[] = nodes.map((node) => {
      const pos = g.node(node.id);
      const visualType = inferNodeType(node, nodes);

      return {
        id: node.id,
        type: "executionNode",
        position: {
          x: pos.x - NODE_WIDTH / 2,
          y: pos.y - NODE_HEIGHT / 2,
        },
        data: {
          node,
          isSelected: node.id === selectedNodeId,
          visualType,
          onFork,
          onMerge,
          onCreateStructuralNode,
          flowMode: flowMode ?? "branching",
          onRunNode: onRunNode ?? (() => {}),
          onUpdateNode: onUpdateNode ?? (() => {}),
          onDeleteNode: onDeleteNode ?? (() => {}),
          onOpenNodeTerminal,
          isOrchestratorTarget: orchestratorCurrentNodeId === node.id,
          debugMode,
          onResetNode,
        },
      };
    });

    // Map to React Flow edges
    const nodeMap = new Map(nodes.map((n) => [n.id, n]));
    const flowEdges: Edge[] = [];

    // Standard parent→child edges (skip agent→merge edges that will be replaced)
    for (const node of nodes) {
      if (!node.parent_id) continue;

      // If this is a merge node whose parent is a decision, skip the decision→merge edge
      // because we'll draw agent→merge edges instead
      if (node.node_type === "merge") {
        const parent = nodeMap.get(node.parent_id);
        if (parent) {
          const siblings = nodes.filter((n) => n.parent_id === node.parent_id);
          const agentSiblings = siblings.filter((n) => n.node_type === "agent");
          if (agentSiblings.length > 0) {
            // Draw edges from each agent sibling → this merge node
            for (const agent of agentSiblings) {
              const variant = inferEdgeVariant(agent, node, ancestryPath, nodes);
              flowEdges.push({
                id: `${agent.id}-${node.id}`,
                source: agent.id,
                target: node.id,
                type: "executionEdge",
                markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
                data: { variant },
              });
            }
            continue; // Skip the normal parent→child edge for this merge node
          }
        }
      }

      const sourceNode = nodeMap.get(node.parent_id)!;
      const variant = inferEdgeVariant(sourceNode, node, ancestryPath, nodes);

      flowEdges.push({
        id: `${node.parent_id}-${node.id}`,
        source: node.parent_id!,
        target: node.id,
        type: "executionEdge",
        markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
        data: { variant },
      });
    }

    return { flowNodes, flowEdges };
  }, [nodes, selectedNodeId, ancestryPath, onFork, onMerge, onCreateStructuralNode, flowMode, onRunNode, onUpdateNode, onDeleteNode, onOpenNodeTerminal, orchestratorCurrentNodeId, debugMode, onResetNode]);
}
