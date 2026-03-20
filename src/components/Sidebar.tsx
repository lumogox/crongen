import { Plus, ChevronRight, Settings, Trash2 } from "lucide-react";
import type { Agent, DecisionNode } from "../types";
import { SessionCard } from "./SessionCard";

interface SidebarProps {
  agents: Agent[];
  selectedAgentId: string | null;
  onSelectAgent: (id: string) => void;
  onNewAgent?: () => void;
  onEditAgent: (agent: Agent) => void;
  onDeleteAgent: (agent: Agent) => void;
  onRunNow: (id: string) => void;
  sessions?: DecisionNode[];
  selectedSessionId?: string | null;
  onSelectSession?: (id: string | null) => void;
  onCreateSession?: () => void;
}

export function Sidebar({
  agents,
  selectedAgentId,
  onSelectAgent,
  onNewAgent,
  onEditAgent,
  onDeleteAgent,
  sessions = [],
  selectedSessionId,
  onSelectSession,
  onCreateSession,
}: SidebarProps) {
  const selectedAgent = agents.find((a) => a.id === selectedAgentId);

  return (
    <aside className="no-select flex h-full flex-col rounded-[1.75rem] border border-white/10 bg-white/[0.03] shadow-xl overflow-hidden">
      {/* Project selector */}
      <div className="border-b border-white/10 px-4 pt-4 pb-3">
        <div className="flex items-center justify-between mb-2">
          <div className="text-[11px] uppercase tracking-[0.18em] text-slate-500">
            Project
          </div>
          {onNewAgent && (
            <button
              onClick={onNewAgent}
              className="flex items-center gap-1 rounded-full border border-white/10 bg-white/5 px-2 py-0.5 text-[11px] font-medium text-slate-400 transition-colors hover:bg-white/10 hover:text-slate-200"
            >
              <Plus className="h-3 w-3" />
              Add
            </button>
          )}
        </div>
        {agents.length === 0 ? (
          <div className="py-3 text-center">
            <p className="text-xs text-slate-500">No projects yet</p>
            <button
              onClick={onNewAgent}
              className="mt-2 text-xs text-sky-400 hover:text-sky-300 transition-colors"
            >
              Create your first project
            </button>
          </div>
        ) : (
          <div className="space-y-1">
            {agents.map((agent) => (
              <div
                key={agent.id}
                className={`group flex w-full items-center gap-2 rounded-xl px-3 py-2 text-left text-sm transition-colors ${
                  agent.id === selectedAgentId
                    ? "bg-white/10 text-slate-50"
                    : "text-slate-400 hover:bg-white/5 hover:text-slate-200"
                }`}
              >
                <button
                  onClick={() => onSelectAgent(agent.id)}
                  className="flex min-w-0 flex-1 items-center gap-2"
                >
                  <ChevronRight className={`h-3 w-3 shrink-0 transition-transform ${agent.id === selectedAgentId ? "rotate-90" : ""}`} />
                  <span className="truncate">{agent.name}</span>
                </button>
                {agent.id === selectedAgentId && (
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={(e) => { e.stopPropagation(); onEditAgent(agent); }}
                      className="rounded-md p-1 text-slate-500 hover:text-slate-200 hover:bg-white/10 transition-colors"
                      title="Edit project"
                    >
                      <Settings className="h-3 w-3" />
                    </button>
                    <button
                      onClick={(e) => { e.stopPropagation(); onDeleteAgent(agent); }}
                      className="rounded-md p-1 text-slate-500 hover:text-rose-300 hover:bg-rose-500/10 transition-colors"
                      title="Delete project"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Session list header */}
      <div className="flex items-center justify-between px-5 pt-4 pb-2">
        <div className="text-lg font-semibold text-slate-50">Sessions</div>
        {selectedAgent && onCreateSession && (
          <button
            onClick={onCreateSession}
            className="flex items-center gap-1 rounded-full border border-white/10 bg-white/5 px-2.5 py-1 text-[11px] font-medium text-slate-300 transition-colors hover:bg-white/10 hover:text-slate-100"
          >
            <Plus className="h-3 w-3" />
            New
          </button>
        )}
      </div>
      <div className="text-sm text-slate-500 px-5 mb-3">
        {selectedAgent ? "Execution flows" : "Select a project"}
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-4 pb-4">
        {!selectedAgent ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center px-6">
              <p className="text-sm text-slate-400">No project selected</p>
              <p className="text-xs text-slate-500 mt-1">
                {agents.length === 0 ? "Create a project first" : "Choose a project above"}
              </p>
            </div>
          </div>
        ) : sessions.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center px-6">
              <p className="text-sm text-slate-400">No sessions yet</p>
              <p className="text-xs text-slate-500 mt-1">
                Create a new task to begin
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            {sessions.map((session) => (
              <SessionCard
                key={session.id}
                session={session}
                isSelected={session.id === selectedSessionId}
                onSelect={() => onSelectSession?.(session.id)}
              />
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}
