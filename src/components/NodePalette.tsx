import { FileCode2, GitFork, Bot, GitMerge, Trophy } from "lucide-react";
import type { VisualNodeType } from "../types/node-types";
import { useDnd } from "./DndContext";

const paletteItems: { type: VisualNodeType; label: string; icon: React.ElementType }[] = [
  { type: "task", label: "Task", icon: FileCode2 },
  { type: "decision", label: "Decision", icon: GitFork },
  { type: "agent", label: "Agent", icon: Bot },
  { type: "merge", label: "Merge", icon: GitMerge },
  { type: "final", label: "Final", icon: Trophy },
];

interface NodePaletteProps {
  flowMode: "linear" | "branching";
}

export function NodePalette({ flowMode }: NodePaletteProps) {
  const { setDragType } = useDnd();

  const items = flowMode === "linear"
    ? paletteItems.filter((p) => p.type === "task" || p.type === "agent")
    : paletteItems;

  return (
    <div className="flex items-center gap-2">
      <span className="text-[11px] uppercase tracking-[0.18em] text-slate-500 mr-1">
        Drag
      </span>
      {items.map((item) => {
        const Icon = item.icon;
        return (
          <div
            key={item.type}
            draggable
            onDragStart={(e) => {
              setDragType(item.type);
              e.dataTransfer.setData("application/crongen-node", item.type);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragEnd={() => setDragType(null)}
            className="flex cursor-grab items-center gap-1.5 rounded-full border border-white/10 bg-white/5 px-2.5 py-1 text-[11px] font-medium text-slate-300 transition-colors hover:bg-white/10 hover:text-slate-100 active:cursor-grabbing"
          >
            <Icon className="h-3 w-3" />
            {item.label}
          </div>
        );
      })}
    </div>
  );
}
