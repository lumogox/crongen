import { Plus, ChevronRight, Settings, Trash2, Play } from "lucide-react";
import type { DecisionNode, Project } from "../types";
import { SessionCard } from "./SessionCard";

interface SidebarProps {
  projects: Project[];
  selectedProjectId: string | null;
  onSelectProject: (id: string) => void;
  onNewProject?: () => void;
  onEditProject: (project: Project) => void;
  onDeleteProject: (project: Project) => void;
  onRunNow: (id: string) => void;
  sessions?: DecisionNode[];
  selectedSessionId?: string | null;
  onSelectSession?: (id: string | null) => void;
  onCreateSession?: () => void;
  onDeleteSession?: (session: DecisionNode) => void;
}

export function Sidebar({
  projects,
  selectedProjectId,
  onSelectProject,
  onNewProject,
  onEditProject,
  onDeleteProject,
  onRunNow,
  sessions = [],
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  onDeleteSession,
}: SidebarProps) {
  const selectedProject = projects.find((project) => project.id === selectedProjectId);

  return (
    <aside className="no-select flex h-full flex-col overflow-hidden rounded-[1.75rem] border border-slate-700/70 bg-[#121a2a] shadow-xl">
      {/* Project selector */}
      <div className="border-b border-slate-700/70 px-4 pt-4 pb-3">
        <div className="flex items-center justify-between mb-2">
          <div className="text-[11px] uppercase tracking-[0.18em] text-slate-400">
            Projects
          </div>
          {onNewProject && (
            <button
              onClick={onNewProject}
              className="flex items-center gap-1 rounded-full border border-slate-600/70 bg-[#182235] px-2 py-0.5 text-[11px] font-medium text-slate-300 transition-colors hover:bg-[#243044] hover:text-slate-100"
            >
              <Plus className="h-3 w-3" />
              Add folder
            </button>
          )}
        </div>
        {projects.length === 0 ? (
          <div className="rounded-xl border border-slate-700/70 bg-[#182235] px-3 py-4 text-center">
            <p className="text-xs font-medium text-slate-300">No project folders</p>
            <p className="mt-1 text-[11px] leading-4 text-slate-400">
              Start new or open existing code.
            </p>
            <button
              onClick={onNewProject}
              className="mt-3 rounded-full border border-sky-400/25 bg-sky-400/10 px-3 py-1 text-xs text-sky-200 transition-colors hover:bg-sky-400/15"
            >
              Add project folder
            </button>
          </div>
        ) : (
          <div className="space-y-1">
            {projects.map((project) => (
              <div
                key={project.id}
                role="button"
                tabIndex={0}
                onClick={() => onSelectProject(project.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelectProject(project.id);
                  }
                }}
                className={`group flex w-full cursor-pointer items-center gap-2 rounded-xl px-3 py-2 text-left text-sm transition-colors focus:outline-none focus:ring-1 focus:ring-sky-400/40 ${
                  project.id === selectedProjectId
                    ? "bg-sky-500/10 text-slate-50 ring-1 ring-sky-400/25"
                    : "text-slate-300 hover:bg-[#182235] hover:text-slate-100"
                }`}
              >
                <div className="flex min-w-0 flex-1 items-center gap-2">
                  <ChevronRight className={`h-3 w-3 shrink-0 transition-transform ${project.id === selectedProjectId ? "rotate-90" : ""}`} />
                  <span className="truncate">{project.name}</span>
                </div>
                {project.id === selectedProjectId && (
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={(e) => { e.stopPropagation(); onRunNow(project.id); }}
                      className="rounded-md p-1 text-slate-400 transition-colors hover:bg-emerald-500/10 hover:text-emerald-300"
                      title="Run project now"
                    >
                      <Play className="h-3 w-3" />
                    </button>
                    <button
                      onClick={(e) => { e.stopPropagation(); onEditProject(project); }}
                      className="rounded-md p-1 text-slate-400 transition-colors hover:bg-[#243044] hover:text-slate-100"
                      title="Edit project"
                    >
                      <Settings className="h-3 w-3" />
                    </button>
                    <button
                      onClick={(e) => { e.stopPropagation(); onDeleteProject(project); }}
                      className="rounded-md p-1 text-slate-400 transition-colors hover:bg-rose-500/10 hover:text-rose-300"
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
        {selectedProject && onCreateSession && (
          <button
            onClick={onCreateSession}
            className="flex items-center gap-1 rounded-full border border-slate-600/70 bg-[#182235] px-2.5 py-1 text-[11px] font-medium text-slate-300 transition-colors hover:bg-[#243044] hover:text-slate-100"
          >
            <Plus className="h-3 w-3" />
            New
          </button>
        )}
      </div>
      <div className="px-5 mb-3 text-sm text-slate-400">
        {selectedProject ? "Execution flows" : "Choose a project folder"}
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-4 pb-4">
        {!selectedProject ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center px-6">
              <p className="text-sm text-slate-400">No project folder selected</p>
              <p className="mt-1 text-xs text-slate-400">
                {projects.length === 0 ? "Add a folder first" : "Choose a folder above"}
              </p>
            </div>
          </div>
        ) : sessions.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center px-6">
              <p className="text-sm text-slate-400">No sessions yet</p>
              <p className="mt-1 text-xs text-slate-400">
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
                onDelete={() => onDeleteSession?.(session)}
              />
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}
