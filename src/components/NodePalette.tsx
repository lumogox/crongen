import { GitFork, Bot, GitMerge, Trophy } from "lucide-react";
import type { VisualNodeType } from "../types/node-types";
import { useDnd } from "./DndContext";

const paletteItems: { type: VisualNodeType; label: string; icon: React.ElementType }[] = [
  { type: "decision", label: "Decision", icon: GitFork },
  { type: "agent", label: "Agent", icon: Bot },
  { type: "merge", label: "Merge", icon: GitMerge },
  { type: "final", label: "Final", icon: Trophy },
];

interface NodePaletteProps {
  flowMode: "linear" | "branching";
  disabled?: boolean;
}

export function NodePalette({ flowMode, disabled = false }: NodePaletteProps) {
  const { setDragType } = useDnd();

  const items = flowMode === "linear"
    ? paletteItems.filter((p) => p.type === "agent")
    : paletteItems;

  return (
    <div className="flex items-center gap-2">
      <span className="mr-1 text-[11px] uppercase tracking-[0.18em] text-slate-400">
        {disabled ? "Select a node" : "Drag"}
      </span>
      {items.map((item) => {
        const Icon = item.icon;
        return (
          <div
            key={item.type}
            draggable={!disabled}
            onDragStart={(e) => {
              if (disabled) {
                e.preventDefault();
                return;
              }
              setDragType(item.type);
              e.dataTransfer.setData("application/crongen-node", item.type);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragEnd={() => setDragType(null)}
            className={`flex items-center gap-1.5 rounded-full border border-slate-700/70 bg-[#182235] px-2.5 py-1 text-[11px] font-medium transition-colors ${
              disabled
                ? "cursor-not-allowed text-slate-500 opacity-60"
                : "cursor-grab text-slate-300 hover:bg-[#243044] hover:text-slate-100 active:cursor-grabbing"
            }`}
          >
            <Icon className="h-3 w-3" />
            {item.label}
          </div>
        );
      })}
    </div>
  );
}
