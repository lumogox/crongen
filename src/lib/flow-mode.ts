import type { DecisionNode } from "../types";

export type FlowMode = "linear" | "branching";

export function inferFlowModeFromNodes(nodes: DecisionNode[]): FlowMode {
  if (nodes.length === 0) {
    return "branching";
  }

  const childrenByParent = new Map<string, number>();

  for (const node of nodes) {
    if (node.node_type === "decision" || node.node_type === "merge" || node.node_type === "final") {
      return "branching";
    }

    if (!node.parent_id) continue;
    childrenByParent.set(node.parent_id, (childrenByParent.get(node.parent_id) ?? 0) + 1);
  }

  for (const count of childrenByParent.values()) {
    if (count > 1) {
      return "branching";
    }
  }

  return "linear";
}
