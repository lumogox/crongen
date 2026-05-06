import { Bot, CheckCircle2, GitFork, GitMerge, Sparkles, Trophy } from "lucide-react";
import type { DecisionNode } from "../types";
import type { PaletteActionType, StructuralNodeType, VisualNodeType } from "../types/node-types";
import { useDnd } from "./DndContext";
import { inferNodeType } from "../lib/node-type-inference";

const paletteItems: {
  type: PaletteActionType;
  label: string;
  group: "Plan" | "Run" | "Branch" | "Resolve";
  icon: React.ElementType;
}[] = [
  { type: "plan", label: "Plan with agent", group: "Plan", icon: Sparkles },
  { type: "agent", label: "Work step", group: "Run", icon: Bot },
  { type: "validation", label: "Validate", group: "Run", icon: CheckCircle2 },
  { type: "decision", label: "Decision", group: "Branch", icon: GitFork },
  { type: "merge", label: "Compare", group: "Resolve", icon: GitMerge },
  { type: "final", label: "Finish", group: "Resolve", icon: Trophy },
];

interface NodePaletteProps {
  selectedNode: DecisionNode | null;
  allNodes: DecisionNode[];
  onAddNode: (nodeType: StructuralNodeType) => void;
  onPlanFromNode: () => void;
}

const allowedByType: Record<VisualNodeType, PaletteActionType[]> = {
  task: ["plan", "agent", "decision", "validation"],
  decision: ["agent"],
  agent: ["plan", "agent", "decision", "merge", "validation"],
  merge: ["final", "validation"],
  final: ["validation"],
  validation: ["agent"],
};

function disabledReason(
  itemType: PaletteActionType,
  selectedType: VisualNodeType | null,
): string | null {
  if (!selectedType) return "Select a node first.";
  if (allowedByType[selectedType].includes(itemType)) return null;

  switch (selectedType) {
    case "decision":
      return "Decision nodes only accept approach work steps.";
    case "merge":
      return "Compare nodes can only be followed by finish or validation steps.";
    case "final":
      return "Finished work can only be validated.";
    case "validation":
      return "Validation can only be followed by a corrective work step.";
    default:
      return "This step is not valid after the selected node.";
  }
}

export function NodePalette({
  selectedNode,
  allNodes,
  onAddNode,
  onPlanFromNode,
}: NodePaletteProps) {
  const { setDragType } = useDnd();
  const selectedType = selectedNode ? inferNodeType(selectedNode, allNodes) : null;

  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="mr-1 text-[11px] uppercase tracking-[0.18em] text-slate-400">Add to selected</span>
      {paletteItems.map((item) => {
        const Icon = item.icon;
        const reason = disabledReason(item.type, selectedType);
        const disabled = Boolean(reason);
        const draggable = !disabled && item.type !== "plan";
        return (
          <button
            key={item.type}
            type="button"
            title={reason ?? (item.type === "plan" ? "Generate child steps under the selected node." : `Add ${item.label.toLowerCase()}. Drag or click.`)}
            disabled={disabled}
            draggable={draggable}
            onClick={() => {
              if (disabled) return;
              if (item.type === "plan") {
                onPlanFromNode();
              } else {
                onAddNode(item.type);
              }
            }}
            onDragStart={(e) => {
              if (!draggable) {
                e.preventDefault();
                return;
              }
              setDragType(item.type as VisualNodeType);
              e.dataTransfer.setData("application/crongen-node", item.type);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragEnd={() => setDragType(null)}
            className={`flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors ${
              disabled
                ? "cursor-not-allowed border-slate-800/80 bg-[#121a2a] text-slate-500 opacity-65"
                : "cursor-pointer border-slate-600/80 bg-[#182235] text-slate-200 hover:border-sky-400/40 hover:bg-[#243044] hover:text-slate-50 active:cursor-grabbing"
            }`}
          >
            <Icon className="h-3 w-3" />
            {item.label}
          </button>
        );
      })}
    </div>
  );
}
