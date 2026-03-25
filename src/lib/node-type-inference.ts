import {
  FileCode2,
  GitFork,
  Bot,
  GitMerge,
  Trophy,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";
import type { DecisionNode } from "../types";
import type { VisualNodeType, EdgeVariant } from "../types/node-types";

interface NodeTypeMeta {
  label: string;
  summary: string;
  icon: LucideIcon;
  accentColor: string;
}

const nodeTypeMeta: Record<VisualNodeType, NodeTypeMeta> = {
  task: { label: "Task", summary: "Root task", icon: FileCode2, accentColor: "#58A6FF" },
  decision: { label: "Decision", summary: "Decision point", icon: GitFork, accentColor: "#D29922" },
  agent: { label: "Agent", summary: "", icon: Bot, accentColor: "#3FB950" },
  merge: { label: "Merge", summary: "Convergence step", icon: GitMerge, accentColor: "#8957E5" },
  final: { label: "Final", summary: "Canonical path", icon: Trophy, accentColor: "#56D364" },
  validation: { label: "Validation", summary: "Main branch check", icon: ShieldCheck, accentColor: "#F2CC60" },
};

export function getNodeTypeMeta(type: VisualNodeType): NodeTypeMeta {
  return nodeTypeMeta[type];
}

const validNodeTypes: VisualNodeType[] = ["task", "decision", "agent", "merge", "final", "validation"];

export function inferNodeType(
  node: DecisionNode,
  allNodes: DecisionNode[],
): VisualNodeType {
  // Prefer explicit node_type if set
  if (node.node_type && validNodeTypes.includes(node.node_type as VisualNodeType)) {
    return node.node_type as VisualNodeType;
  }

  // Legacy heuristic fallback for nodes without node_type
  // Merged node
  if (node.status === "merged") return "merge";

  // Build children map
  const childrenMap = new Map<string, DecisionNode[]>();
  for (const n of allNodes) {
    if (n.parent_id) {
      const siblings = childrenMap.get(n.parent_id) ?? [];
      siblings.push(n);
      childrenMap.set(n.parent_id, siblings);
    }
  }

  const isRoot = node.parent_id === null;
  const children = childrenMap.get(node.id) ?? [];
  const isLeaf = children.length === 0;

  // Root node → task
  if (isRoot) {
    // Sole completed root in a linear tree (no forks) → final
    if (
      isLeaf &&
      node.status === "completed" &&
      allNodes.length === 1
    ) {
      return "final";
    }
    return "task";
  }

  // Fork point: node with 2+ children
  if (children.length >= 2) return "decision";

  // Completed leaf whose parent is merged → final
  if (isLeaf && node.status === "completed" && node.parent_id) {
    const parent = allNodes.find((n) => n.id === node.parent_id);
    if (parent?.status === "merged") return "final";
  }

  // Leaf node → agent (default for leaves)
  if (isLeaf) return "agent";

  // Fallback
  return "agent";
}

export function inferEdgeVariant(
  sourceNode: DecisionNode,
  targetNode: DecisionNode,
  ancestryPath: Set<string>,
  allNodes: DecisionNode[],
): EdgeVariant {
  // Both in ancestry path → active
  if (ancestryPath.has(sourceNode.id) && ancestryPath.has(targetNode.id)) {
    return "active";
  }

  // Source is a decision type (fork point with 2+ children)
  const sourceChildren = allNodes.filter(
    (n) => n.parent_id === sourceNode.id,
  );
  if (sourceChildren.length >= 2) {
    return "speculative";
  }

  // Target is pending/queued
  if (targetNode.status === "pending") {
    return "waiting";
  }

  return "default";
}
