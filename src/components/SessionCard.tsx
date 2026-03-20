import { ChevronRight, Clock3, Play, CheckCircle2, XCircle, Pause, GitMerge } from "lucide-react";
import type { DecisionNode, NodeStatus } from "../types";
import { formatRelativeTime } from "../lib/utils";

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
}

export function SessionCard({ session, isSelected, onSelect }: SessionCardProps) {
  const status = statusConfig[session.status];
  const StatusIcon = status.icon;

  return (
    <button
      onClick={onSelect}
      className={`flex w-full items-start gap-3 rounded-2xl border p-3 text-left transition-all ${
        isSelected
          ? "border-sky-400/30 bg-sky-500/10"
          : "border-white/10 bg-white/[0.02] hover:bg-white/5"
      }`}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium text-slate-100">
            {session.label}
          </span>
          <StatusIcon className={`h-3.5 w-3.5 shrink-0 ${status.tone}`} />
        </div>
        {session.prompt && (
          <div className="mt-1 truncate text-xs text-slate-500">
            {session.prompt}
          </div>
        )}
        <div className="mt-1.5 text-[11px] text-slate-600">
          {formatRelativeTime(session.created_at)}
        </div>
      </div>
      <ChevronRight className={`mt-0.5 h-4 w-4 shrink-0 transition-transform ${isSelected ? "text-sky-300 rotate-90" : "text-slate-600"}`} />
    </button>
  );
}
