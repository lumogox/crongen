import { useEffect, useMemo, useState } from "react";
import type { AgentProviderReadiness, AgentRole, AgentType, AppSettings } from "../types";
import { BUILT_IN_AGENT_TYPES, getAgentLabel } from "../lib/agent-templates";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Bot,
  Brain,
  CheckCircle2,
  Loader2,
  Orbit,
  RefreshCw,
  Sparkles,
  TriangleAlert,
  Zap,
} from "lucide-react";

interface SettingsModalProps {
  settings: AppSettings;
  statuses: AgentProviderReadiness[];
  forceSetup?: boolean;
  onboarding?: boolean;
  focusRole?: AgentRole;
  onSave: (settings: AppSettings) => Promise<void> | void;
  onRefreshStatuses: () => Promise<void> | void;
  onClose: () => void;
}

type ProviderTheme = {
  icon: typeof Brain;
  tint: string;
  surface: string;
  ring: string;
  description: string;
  chips: string[];
};

const PROVIDER_THEMES: Record<AgentType, ProviderTheme> = {
  claude_code: {
    icon: Brain,
    tint: "text-violet-200",
    surface: "from-violet-500/18 via-violet-500/6 to-transparent",
    ring: "border-violet-400/30",
    description: "Anthropic's coding agent with SDK-backed runs for stable task execution.",
    chips: ["Planning", "Execution", "SDK"],
  },
  codex: {
    icon: Bot,
    tint: "text-cyan-200",
    surface: "from-cyan-500/18 via-sky-500/6 to-transparent",
    ring: "border-cyan-400/30",
    description: "OpenAI's coding agent with terminal-native workflows and strong repo awareness.",
    chips: ["Planning", "Execution", "Terminal"],
  },
  gemini: {
    icon: Orbit,
    tint: "text-amber-100",
    surface: "from-amber-500/18 via-orange-500/6 to-transparent",
    ring: "border-amber-400/30",
    description: "Planned future adapter for Google's Gemini CLI. Visible here so the bay scales cleanly.",
    chips: ["Future", "Coming soon"],
  },
  custom: {
    icon: Sparkles,
    tint: "text-slate-200",
    surface: "from-white/10 via-white/5 to-transparent",
    ring: "border-white/10",
    description: "Custom providers are not part of Agent Bay defaults.",
    chips: ["Custom"],
  },
};

function statusTone(status: AgentProviderReadiness["status"]) {
  switch (status) {
    case "ready":
      return "border-emerald-400/20 bg-emerald-500/10 text-emerald-200";
    case "needs_login":
      return "border-amber-400/20 bg-amber-500/10 text-amber-100";
    case "missing_cli":
    case "error":
      return "border-rose-400/20 bg-rose-500/10 text-rose-100";
    case "coming_soon":
      return "border-slate-400/20 bg-slate-500/10 text-slate-300";
    default:
      return "border-white/10 bg-white/5 text-slate-200";
  }
}

function statusLabel(status: AgentProviderReadiness["status"]) {
  switch (status) {
    case "ready":
      return "Ready";
    case "needs_login":
      return "Needs login";
    case "missing_cli":
      return "Install CLI";
    case "coming_soon":
      return "Coming soon";
    case "error":
      return "Check failed";
    default:
      return "Unknown";
  }
}

function AgentStatusBadge({ status }: { status: AgentProviderReadiness["status"] }) {
  return (
    <span className={`rounded-full border px-2 py-0.5 text-[10px] uppercase tracking-[0.18em] ${statusTone(status)}`}>
      {statusLabel(status)}
    </span>
  );
}

function RoleSocket({
  role,
  focus,
  selectedAgent,
  modelValue,
  onModelChange,
  status,
}: {
  role: AgentRole;
  focus: boolean;
  selectedAgent: AgentType | null | undefined;
  modelValue: string;
  onModelChange: (value: string) => void;
  status: AgentProviderReadiness | null;
}) {
  const roleLabel = role === "planning" ? "Planning" : "Execution";
  const roleDescription = role === "planning" ? "Task decomposition" : "Agent node runs";

  return (
    <div
      className={`rounded-[1.4rem] border bg-black/30 p-4 transition-all ${
        focus
          ? "border-sky-400/35 shadow-[0_0_0_1px_rgba(56,189,248,0.18),0_18px_50px_rgba(14,165,233,0.12)]"
          : "border-white/10"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[11px] uppercase tracking-[0.24em] text-slate-500">{roleLabel}</div>
          <div className="mt-1 text-sm font-medium text-slate-100">{getAgentLabel(selectedAgent)}</div>
          <div className="mt-1 text-xs text-slate-500">{roleDescription}</div>
        </div>
        {status ? <AgentStatusBadge status={status.status} /> : (
          <span className="rounded-full border border-white/10 bg-white/5 px-2 py-0.5 text-[10px] uppercase tracking-[0.18em] text-slate-400">
            Unset
          </span>
        )}
      </div>

      <div className="mt-4 space-y-2">
        <label className="block text-[11px] uppercase tracking-[0.18em] text-slate-500">
          Model override
        </label>
        <Input
          value={modelValue}
          onChange={(event) => onModelChange(event.target.value)}
          placeholder={selectedAgent ? `${getAgentLabel(selectedAgent)} default` : "Select a provider below"}
          className="border-white/10 bg-black/30 text-slate-100 placeholder:text-slate-600"
        />
      </div>

      <div className="mt-3 text-xs text-slate-500">
        {status?.detail ?? "Select a provider below to wire this role."}
      </div>
    </div>
  );
}

export function SettingsModal({
  settings,
  statuses,
  forceSetup = false,
  onboarding = false,
  focusRole,
  onSave,
  onRefreshStatuses,
  onClose,
}: SettingsModalProps) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [isSaving, setIsSaving] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const statusByType = useMemo(
    () => new Map(statuses.map((status) => [status.agent_type, status])),
    [statuses],
  );

  const planningStatus = draft.planning_agent ? statusByType.get(draft.planning_agent) ?? null : null;
  const executionStatus = draft.execution_agent ? statusByType.get(draft.execution_agent) ?? null : null;
  const hasRequiredDefaults = Boolean(draft.planning_agent && draft.execution_agent);
  const saveDisabled = isSaving || (onboarding && !hasRequiredDefaults);

  async function handleSave() {
    setIsSaving(true);
    try {
      await onSave({
        ...draft,
        agent_setup_seen: settings.agent_setup_seen || hasRequiredDefaults,
        planning_model: draft.planning_model?.trim() || null,
        execution_model: draft.execution_model?.trim() || null,
      });
      onClose();
    } finally {
      setIsSaving(false);
    }
  }

  async function handleSkip() {
    setIsSaving(true);
    try {
      await onSave({
        ...settings,
        agent_setup_seen: true,
      });
      onClose();
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRefresh() {
    setIsRefreshing(true);
    try {
      await onRefreshStatuses();
    } finally {
      setIsRefreshing(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open && !forceSetup) onClose(); }}>
      <DialogContent
        showCloseButton={!forceSetup}
        className="agent-bay-shell max-w-[calc(100%-1.5rem)] border-white/10 bg-[#050816]/95 p-0 shadow-[0_40px_120px_rgba(2,6,23,0.82)] sm:max-w-5xl"
      >
        <div className="agent-bay-scanlines relative overflow-hidden rounded-[1.35rem]">
          <div className="absolute inset-0 bg-[radial-gradient(circle_at_top,rgba(14,165,233,0.12),transparent_34%),radial-gradient(circle_at_bottom_right,rgba(168,85,247,0.12),transparent_32%)]" />

          <div className="relative p-6 sm:p-7">
            <DialogHeader className="space-y-3 text-left">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <div className="text-[11px] uppercase tracking-[0.26em] text-sky-300/80">
                    Agent Bay
                  </div>
                  <DialogTitle className="mt-2 text-2xl text-slate-50">
                    {onboarding ? "Connect your first agent" : "Default provider cockpit"}
                  </DialogTitle>
                  <DialogDescription className="mt-2 max-w-2xl text-sm text-slate-400">
                    {onboarding
                      ? "Pick a planning agent and an execution agent, validate them, and save your defaults before you start routing work."
                      : "Choose which agent decomposes tasks and which agent new projects inherit for execution. Gemini stays visible here as the future slot."}
                  </DialogDescription>
                </div>

                <Button
                  variant="outline"
                  onClick={handleRefresh}
                  disabled={isRefreshing || isSaving}
                  className="rounded-full border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                >
                  {isRefreshing ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <RefreshCw className="h-4 w-4" />
                  )}
                  Validate
                </Button>
              </div>

              <div className="grid gap-3 sm:grid-cols-2">
                <RoleSocket
                  role="planning"
                  focus={focusRole === "planning"}
                  selectedAgent={draft.planning_agent}
                  modelValue={draft.planning_model ?? ""}
                  onModelChange={(value) => setDraft((current) => ({ ...current, planning_model: value }))}
                  status={planningStatus}
                />
                <RoleSocket
                  role="execution"
                  focus={focusRole === "execution"}
                  selectedAgent={draft.execution_agent}
                  modelValue={draft.execution_model ?? ""}
                  onModelChange={(value) => setDraft((current) => ({ ...current, execution_model: value }))}
                  status={executionStatus}
                />
              </div>
            </DialogHeader>

            <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,0.9fr)]">
              <div className="rounded-[1.2rem] border border-white/10 bg-black/25 p-4">
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <div className="text-[11px] uppercase tracking-[0.22em] text-slate-500">
                      Diagnostics
                    </div>
                    <div className="mt-2 text-sm font-medium text-slate-100">Debug mode</div>
                    <div className="mt-1 text-xs text-slate-500">
                      Reveal reset controls on completed nodes and keep the TOON context viewer available.
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => setDraft((current) => ({ ...current, debug_mode: !current.debug_mode }))}
                    className={`rounded-full border px-3 py-1 text-[11px] uppercase tracking-[0.18em] transition-colors ${
                      draft.debug_mode
                        ? "border-emerald-400/30 bg-emerald-500/10 text-emerald-200"
                        : "border-white/10 bg-white/5 text-slate-400 hover:bg-white/10"
                    }`}
                  >
                    {draft.debug_mode ? "Enabled" : "Disabled"}
                  </button>
                </div>
              </div>

              {onboarding && !hasRequiredDefaults && (
                <div className="rounded-[1.2rem] border border-amber-400/20 bg-amber-500/10 p-4 text-sm text-amber-100">
                  <div className="flex items-start gap-2">
                    <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
                    <div>
                      <div className="font-medium text-amber-50">Choose both role defaults before saving</div>
                      <div className="mt-1 text-xs text-amber-100/80">
                        Save stays locked until planning and execution each have a selected provider. You can still skip for now and come back later.
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </div>

            <div className="mt-6 grid gap-3 lg:grid-cols-3">
              {BUILT_IN_AGENT_TYPES.map((provider) => {
                const theme = PROVIDER_THEMES[provider];
                const status = statusByType.get(provider) ?? null;
                const Icon = theme.icon;
                const selectedForPlanning = draft.planning_agent === provider;
                const selectedForExecution = draft.execution_agent === provider;
                const disabled = status?.coming_soon || !status?.supports_planning && !status?.supports_execution;

                return (
                  <section
                    key={provider}
                    className={`relative overflow-hidden rounded-[1.45rem] border bg-gradient-to-br ${theme.surface} ${
                      selectedForPlanning || selectedForExecution ? theme.ring : "border-white/10"
                    }`}
                  >
                    <div className="absolute inset-0 bg-[linear-gradient(180deg,rgba(255,255,255,0.03),transparent_42%)]" />
                    <div className="relative flex h-full flex-col p-4">
                      <div className="flex items-start justify-between gap-3">
                        <div className="flex items-center gap-3">
                          <div className={`rounded-2xl border border-white/10 bg-black/30 p-2.5 ${theme.tint}`}>
                            <Icon className="h-5 w-5" />
                          </div>
                          <div>
                            <div className="text-base font-semibold text-slate-50">{getAgentLabel(provider)}</div>
                            <div className="mt-1 text-xs text-slate-500">{theme.description}</div>
                          </div>
                        </div>
                        {status ? <AgentStatusBadge status={status.status} /> : null}
                      </div>

                      <div className="mt-4 flex flex-wrap gap-2">
                        {theme.chips.map((chip) => (
                          <span
                            key={chip}
                            className="rounded-full border border-white/10 bg-black/20 px-2 py-0.5 text-[10px] uppercase tracking-[0.18em] text-slate-400"
                          >
                            {chip}
                          </span>
                        ))}
                      </div>

                      <div className="mt-4 flex-1 rounded-2xl border border-white/10 bg-black/20 p-3 text-xs text-slate-400">
                        {status?.detail ?? "No validation details yet."}
                      </div>

                      {(selectedForPlanning || selectedForExecution) && (
                        <div className="mt-3 flex flex-wrap gap-2">
                          {selectedForPlanning && (
                            <span className="rounded-full border border-sky-400/20 bg-sky-500/10 px-2 py-0.5 text-[10px] uppercase tracking-[0.18em] text-sky-200">
                              Planning selected
                            </span>
                          )}
                          {selectedForExecution && (
                            <span className="rounded-full border border-emerald-400/20 bg-emerald-500/10 px-2 py-0.5 text-[10px] uppercase tracking-[0.18em] text-emerald-200">
                              Execution selected
                            </span>
                          )}
                        </div>
                      )}

                      <div className="mt-4 flex flex-wrap gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={disabled || isSaving}
                          onClick={() => setDraft((current) => ({ ...current, planning_agent: provider }))}
                          className="rounded-full border-white/10 bg-black/20 text-slate-100 hover:bg-white/10"
                        >
                          <Brain className="h-3.5 w-3.5" />
                          Use for planning
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={disabled || isSaving}
                          onClick={() => setDraft((current) => ({ ...current, execution_agent: provider }))}
                          className="rounded-full border-white/10 bg-black/20 text-slate-100 hover:bg-white/10"
                        >
                          <Zap className="h-3.5 w-3.5" />
                          Use for execution
                        </Button>
                      </div>

                      {status?.status === "needs_login" && (
                        <div className="mt-3 flex items-start gap-2 rounded-2xl border border-amber-400/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-100">
                          <TriangleAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                          <span>Validate after you complete the provider's login flow.</span>
                        </div>
                      )}

                      {status?.status === "ready" && (
                        <div className="mt-3 flex items-center gap-2 rounded-2xl border border-emerald-400/20 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-200">
                          <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
                          <span>This provider is cleared for Agent Bay roles.</span>
                        </div>
                      )}
                    </div>
                  </section>
                );
              })}
            </div>

            <div className="mt-6 flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
              {onboarding ? (
                <Button
                  variant="ghost"
                  onClick={handleSkip}
                  disabled={isSaving}
                  className="rounded-full text-slate-400 hover:bg-white/5 hover:text-slate-100"
                >
                  Skip for now
                </Button>
              ) : (
                <Button
                  variant="ghost"
                  onClick={onClose}
                  disabled={isSaving}
                  className="rounded-full text-slate-400 hover:bg-white/5 hover:text-slate-100"
                >
                  Cancel
                </Button>
              )}

              <Button
                onClick={handleSave}
                disabled={saveDisabled}
                className="rounded-full bg-sky-500 text-slate-950 hover:bg-sky-400"
              >
                {isSaving ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Saving
                  </>
                ) : (
                  <>
                    <Sparkles className="h-4 w-4" />
                    Save defaults
                  </>
                )}
              </Button>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
