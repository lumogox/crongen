import { useState, useRef, useEffect } from "react";
import { ChevronRight } from "lucide-react";
import type { Agent } from "../types";
import { formatRelativeTime } from "../lib/utils";

interface AgentCardProps {
  agent: Agent;
  isSelected: boolean;
  onSelect: (id: string) => void;
  onEdit: (agent: Agent) => void;
  onDelete: (agent: Agent) => void;
  onRunNow: (id: string) => void;
}

export function AgentCard({
  agent,
  isSelected,
  onSelect,
  onEdit,
  onDelete,
  onRunNow,
}: AgentCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  return (
    <div className="relative">
      <button
        className={`w-full rounded-2xl border px-4 py-3 text-left transition-all ${
          isSelected
            ? "border-sky-400/30 bg-sky-500/10"
            : "border-white/10 bg-white/[0.03] hover:bg-white/[0.05]"
        }`}
        onClick={() => onSelect(agent.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenuOpen(true);
        }}
      >
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-sm font-medium text-slate-100 truncate">
              {agent.name}
            </div>
            <div className="mt-1 text-xs text-slate-500">
              {agent.agent_type === "claude_code"
                ? "Claude Code"
                : agent.agent_type === "codex"
                  ? "Codex"
                  : agent.agent_type === "gemini"
                    ? "Gemini"
                    : "Custom"}
            </div>
          </div>
          <ChevronRight className="h-4 w-4 shrink-0 text-slate-500" />
        </div>
        <div className="mt-3 text-xs text-slate-500">
          {formatRelativeTime(agent.updated_at)}
        </div>
      </button>

      {/* Context menu */}
      {menuOpen && (
        <div
          ref={menuRef}
          className="absolute right-2 top-2 z-20 min-w-[130px] rounded-2xl border border-white/10 bg-slate-950 py-1.5 shadow-xl"
        >
          <MenuButton
            onClick={() => {
              setMenuOpen(false);
              onEdit(agent);
            }}
          >
            Edit
          </MenuButton>
          <MenuButton
            onClick={() => {
              setMenuOpen(false);
              onRunNow(agent.id);
            }}
          >
            Run Now
          </MenuButton>
          <div className="my-1 border-t border-white/10" />
          <MenuButton
            danger
            onClick={() => {
              setMenuOpen(false);
              onDelete(agent);
            }}
          >
            Delete
          </MenuButton>
        </div>
      )}
    </div>
  );
}

function MenuButton({
  children,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      className={`w-full px-3 py-1.5 text-left text-[12px] transition-colors ${
        danger
          ? "text-rose-300 hover:bg-rose-500/10"
          : "text-slate-200 hover:bg-white/5"
      }`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
