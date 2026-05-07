import {
  ChevronRight,
  Clock3,
  Play,
  CheckCircle2,
  XCircle,
  Pause,
  GitMerge,
  Trash2,
} from "lucide-react";
import type { DecisionNode, NodeStatus } from "../types";
import { formatSessionRuntime } from "../lib/utils";

const statusConfig: Record<NodeStatus, { icon: React.ElementType; label: string; tone: string }> = {
  pending: { icon: Clock3, label: "Pending", tone: "text-slate-400" },
  running: { icon: Play, label: "Running", tone: "text-amber-300" },
  paused: { icon: Pause, label: "Paused", tone: "text-sky-300" },
  completed: { icon: CheckCircle2, label: "Done", tone: "text-emerald-300" },
  failed: { icon: XCircle, label: "Failed", tone: "text-rose-300" },
  merged: { icon: GitMerge, label: "Merged", tone: "text-violet-300" },
};

interface SessionCardProps {
  session: DecisionNode;
  isSelected: boolean;
  onSelect: () => void;
  onDelete?: () => void;
}

export function SessionCard({ session, isSelected, onSelect, onDelete }: SessionCardProps) {
  const status = statusConfig[session.status];
  const StatusIcon = status.icon;
  const deleteDisabled = session.status === "running" || session.status === "paused";

  return (
    <div
      className={`group flex w-full items-start gap-2 rounded-2xl border p-2 text-left transition-all ${
        isSelected
          ? "border-sky-400/30 bg-sky-500/10"
          : "border-slate-700/70 bg-[#182235] hover:bg-[#243044]"
      }`}
    >
      <button
        onClick={onSelect}
        className="flex min-w-0 flex-1 items-start gap-3 rounded-xl p-1 text-left"
      >
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-medium text-slate-100">
              {session.label}
            </span>
            <StatusIcon className={`h-3.5 w-3.5 shrink-0 ${status.tone}`} />
          </div>
          {session.prompt && (
            <div className="mt-1 truncate text-xs text-slate-400">
              {session.prompt}
            </div>
          )}
          <div className="mt-1.5 text-[11px] text-slate-500">
            {formatSessionRuntime(session)}
          </div>
        </div>
        <ChevronRight
          className={`mt-0.5 h-4 w-4 shrink-0 transition-transform ${
            isSelected ? "rotate-90 text-sky-300" : "text-slate-500"
          }`}
        />
      </button>
      {onDelete && (
        <button
          aria-label={`Delete ${session.label}`}
          onClick={(event) => {
            event.stopPropagation();
            if (!deleteDisabled) onDelete();
          }}
          disabled={deleteDisabled}
          title={deleteDisabled ? "Stop the session before deleting" : "Delete session"}
          className="mt-1 rounded-lg p-1.5 text-slate-500 opacity-0 transition-all hover:bg-rose-500/10 hover:text-rose-300 disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-slate-500 group-hover:opacity-100 focus:opacity-100"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}
