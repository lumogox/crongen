import { useEffect, useMemo, useState } from "react";
import type {
  AgentProviderReadiness,
  AgentRole,
  AgentType,
  AppSettings,
  ClaudeCodeConfig,
  CodexConfig,
  GeminiConfig,
} from "../types";
import {
  AGENT_TEMPLATES,
  BUILT_IN_AGENT_TYPES,
  createDefaultAgentConfigs,
  getAgentLabel,
} from "../lib/agent-templates";
import { Dialog } from "@/components/ui/dialog";
import {
  AppModalBody,
  AppModalContent,
  AppModalFooter,
  AppModalHeader,
} from "@/components/ui/app-modal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip } from "@/components/ui/tooltip";
import {
  Bot,
  Brain,
  CircleDashed,
  Settings,
  ArrowLeft,
  Loader2,
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

type ProviderSummary = {
  icon: typeof Brain;
  description: string;
  bestFor: string;
  accent: string;
};

const PROVIDER_SUMMARIES: Record<AgentType, ProviderSummary> = {
  claude_code: {
    icon: Brain,
    description: "Claude Code CLI",
    bestFor: "Stable SDK-backed task work",
    accent: "text-violet-200",
  },
  codex: {
    icon: Bot,
    description: "OpenAI Codex CLI",
    bestFor: "Local SDK-backed task work",
    accent: "text-cyan-200",
  },
  gemini: {
    icon: Sparkles,
    description: "Gemini CLI",
    bestFor: "Planned future provider",
    accent: "text-amber-100",
  },
  custom: {
    icon: CircleDashed,
    description: "Project-specific command",
    bestFor: "Configured on a project",
    accent: "text-slate-200",
  },
};

function statusTone(status: AgentProviderReadiness["status"]) {
  switch (status) {
    case "ready":
      return "border-emerald-400/25 bg-emerald-500/10 text-emerald-200";
    case "needs_login":
      return "border-amber-400/25 bg-amber-500/10 text-amber-100";
    case "missing_cli":
    case "error":
      return "border-rose-400/25 bg-rose-500/10 text-rose-100";
    case "coming_soon":
      return "border-slate-400/20 bg-slate-500/10 text-slate-300";
    default:
      return "border-slate-600/70 bg-[#182235] text-slate-200";
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
    <span className={`rounded-full border px-2 py-0.5 text-[10px] uppercase tracking-[0.14em] ${statusTone(status)}`}>
      {statusLabel(status)}
    </span>
  );
}

function canUseProviderForRole(provider: AgentType, status: AgentProviderReadiness | null, role: AgentRole) {
  if (!status) return provider !== "custom";
  if (status.coming_soon) return false;
  return role === "planning" ? status.supports_planning : status.supports_execution;
}

function providerHelpText(status: AgentProviderReadiness | null) {
  if (!status) return "Validation unavailable right now; you can still choose it.";
  if (status.status === "ready") return "Ready to use.";
  return status.detail ?? statusLabel(status.status);
}

function normalizeExtraArgs(args: string[] | undefined): string[] {
  return (args ?? []).map((arg) => arg.trim()).filter(Boolean);
}

function normalizeSettings(settings: AppSettings): AppSettings {
  const defaults = createDefaultAgentConfigs();
  return {
    ...settings,
    agent_configs: {
      claude_code: {
        ...(defaults.claude_code as ClaudeCodeConfig),
        ...(settings.agent_configs?.claude_code ?? {}),
        extra_args: normalizeExtraArgs(settings.agent_configs?.claude_code?.extra_args),
      },
      codex: {
        ...(defaults.codex as CodexConfig),
        ...(settings.agent_configs?.codex ?? {}),
        extra_args: normalizeExtraArgs(settings.agent_configs?.codex?.extra_args),
      },
      gemini: {
        ...(defaults.gemini as GeminiConfig),
        ...(settings.agent_configs?.gemini ?? {}),
        extra_args: normalizeExtraArgs(settings.agent_configs?.gemini?.extra_args),
      },
    },
  };
}

function argsToText(args: string[] | undefined): string {
  return (args ?? []).join("\n");
}

function textToArgs(value: string): string[] {
  return value.split("\n").map((arg) => arg.trim()).filter(Boolean);
}

function configForProvider(settings: AppSettings, provider: AgentType) {
  switch (provider) {
    case "claude_code":
      return settings.agent_configs?.claude_code ?? null;
    case "codex":
      return settings.agent_configs?.codex ?? null;
    case "gemini":
      return settings.agent_configs?.gemini ?? null;
    case "custom":
      return null;
  }
}

function CurrentRoleCard({
  role,
  focus,
  selectedAgent,
  status,
}: {
  role: AgentRole;
  focus: boolean;
  selectedAgent: AgentType | null | undefined;
  status: AgentProviderReadiness | null;
}) {
  const Icon = role === "planning" ? Brain : Zap;
  const roleLabel = role === "planning" ? "Planning" : "Execution";
  const roleDescription = role === "planning" ? "Breaks a task into a plan" : "Runs each work node";

  return (
    <div
      className={`rounded-lg border bg-[#182235] p-3 ${
        focus ? "border-sky-400/60 shadow-[0_0_0_1px_rgba(56,189,248,0.22)]" : "border-slate-700/70"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <div className="rounded-lg border border-slate-600/70 bg-[#243044] p-2 text-sky-200">
            <Icon className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="text-[11px] uppercase tracking-[0.18em] text-slate-400">{roleLabel}</div>
            <div className="mt-1 truncate text-sm font-semibold text-slate-50">
              {selectedAgent ? getAgentLabel(selectedAgent) : "Choose an agent"}
            </div>
            <div className="mt-0.5 text-xs text-slate-400">{roleDescription}</div>
          </div>
        </div>
        {status ? (
          <AgentStatusBadge status={status.status} />
        ) : (
          <span className="rounded-full border border-slate-600/70 bg-[#243044] px-2 py-0.5 text-[10px] uppercase tracking-[0.14em] text-slate-400">
            {selectedAgent ? "Not validated" : "Unset"}
          </span>
        )}
      </div>
    </div>
  );
}

function ProviderRow({
  provider,
  status,
  selectedForPlanning,
  selectedForExecution,
  isSaving,
  onUseBoth,
  onUsePlanning,
  onUseExecution,
}: {
  provider: AgentType;
  status: AgentProviderReadiness | null;
  selectedForPlanning: boolean;
  selectedForExecution: boolean;
  isSaving: boolean;
  onUseBoth: () => void;
  onUsePlanning: () => void;
  onUseExecution: () => void;
}) {
  const summary = PROVIDER_SUMMARIES[provider];
  const Icon = summary.icon;
  const canPlan = canUseProviderForRole(provider, status, "planning");
  const canExecute = canUseProviderForRole(provider, status, "execution");
  const canUseBoth = canPlan && canExecute;
  const selected = selectedForPlanning || selectedForExecution;
  const selectedForBoth = selectedForPlanning && selectedForExecution;

  return (
    <section
      className={`rounded-lg border bg-[#121a2a] p-3 transition-colors ${
        selected ? "border-sky-400/50 bg-sky-500/10" : "border-slate-700/70"
      }`}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <div className={`rounded-lg border border-slate-600/70 bg-[#243044] p-2.5 ${summary.accent}`}>
            <Icon className="h-5 w-5" />
          </div>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <div className="text-sm font-semibold text-slate-50">{getAgentLabel(provider)}</div>
              {status ? <AgentStatusBadge status={status.status} /> : null}
            </div>
            <div className="mt-1 text-xs text-slate-400">{summary.description} · {summary.bestFor}</div>
            <div className="mt-1 text-xs text-slate-400">{providerHelpText(status)}</div>
          </div>
        </div>

        <div className="flex shrink-0 flex-wrap gap-2 sm:justify-end">
          <Button
            variant={canUseBoth ? "default" : "outline"}
            size="sm"
            disabled={!canUseBoth || isSaving}
            onClick={onUseBoth}
            className={
              selectedForBoth
                ? "rounded-lg border border-sky-300/60 bg-sky-400 text-slate-950 shadow-[0_0_0_1px_rgba(125,211,252,0.25)] hover:bg-sky-300"
                : canUseBoth
                ? "rounded-lg bg-sky-500 text-slate-950 hover:bg-sky-400"
                : "rounded-lg border-slate-700/70 bg-[#182235] text-slate-500"
            }
          >
            Use for both
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={!canPlan || isSaving}
            onClick={onUsePlanning}
            className={
              selectedForPlanning
                ? "rounded-lg border-sky-300/50 bg-sky-500/20 text-sky-50 shadow-[0_0_0_1px_rgba(125,211,252,0.18)] hover:bg-sky-500/25"
                : "rounded-lg border-slate-700/70 bg-[#182235] text-slate-100 hover:bg-[#243044] disabled:text-slate-500"
            }
          >
            <Brain className="h-3.5 w-3.5" />
            Planning
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={!canExecute || isSaving}
            onClick={onUseExecution}
            className={
              selectedForExecution
                ? "rounded-lg border-emerald-300/50 bg-emerald-500/20 text-emerald-50 shadow-[0_0_0_1px_rgba(110,231,183,0.18)] hover:bg-emerald-500/25"
                : "rounded-lg border-slate-700/70 bg-[#182235] text-slate-100 hover:bg-[#243044] disabled:text-slate-500"
            }
          >
            <Zap className="h-3.5 w-3.5" />
            Execution
          </Button>
        </div>
      </div>
    </section>
  );
}

function ProviderConfigPanel({
  provider,
  config,
  onModelChange,
  onArgsChange,
}: {
  provider: AgentType;
  config: ClaudeCodeConfig | CodexConfig | GeminiConfig;
  onModelChange: (value: string) => void;
  onArgsChange: (value: string) => void;
}) {
  const commandPreview = AGENT_TEMPLATES[provider].buildCommandPreview(
    "Run the selected task",
    config,
  );

  return (
    <div className="rounded-lg border border-slate-700/70 bg-[#182235] p-3">
      <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
        <label className="grid gap-1.5">
          <span className="text-[11px] uppercase tracking-[0.16em] text-slate-400">Model</span>
          <Input
            value={config.model ?? ""}
            onChange={(event) => onModelChange(event.target.value)}
            placeholder="Leave blank for CLI default"
            className="border-slate-600/70 bg-[#0f1726] text-slate-100 placeholder:text-slate-500"
          />
        </label>
        <label className="grid gap-1.5">
          <span className="text-[11px] uppercase tracking-[0.16em] text-slate-400">Extra CLI args</span>
          <Textarea
            value={argsToText(config.extra_args)}
            onChange={(event) => onArgsChange(event.target.value)}
            placeholder={"One argument per line\ne.g. --include-directories\n../shared"}
            rows={3}
            className="min-h-24 resize-none border-slate-600/70 bg-[#0f1726] text-slate-100 placeholder:text-slate-500"
          />
        </label>
      </div>
      <div className="mt-3 rounded-md border border-slate-700/70 bg-[#0f1726] p-2">
        <div className="mb-1 text-[10px] uppercase tracking-[0.16em] text-slate-400">Preview</div>
        <code className="block whitespace-pre-wrap break-words text-[11px] leading-5 text-slate-300">
          {commandPreview}
        </code>
      </div>
    </div>
  );
}

function CliSettingsView({
  draft,
  statuses,
  onModelChange,
  onArgsChange,
}: {
  draft: AppSettings;
  statuses: Map<AgentType, AgentProviderReadiness>;
  onModelChange: (provider: AgentType, value: string) => void;
  onArgsChange: (provider: AgentType, value: string) => void;
}) {
  const normalizedDraft = normalizeSettings(draft);

  return (
    <div className="space-y-3">
      {BUILT_IN_AGENT_TYPES.map((provider) => {
        const summary = PROVIDER_SUMMARIES[provider];
        const Icon = summary.icon;
        const config = configForProvider(normalizedDraft, provider);
        const status = statuses.get(provider);
        if (!config) return null;

        return (
          <section key={provider} className="rounded-lg border border-slate-700/70 bg-[#121a2a] p-3">
            <div className="mb-3 flex items-start justify-between gap-3">
              <div className="flex min-w-0 items-start gap-3">
                <div className={`rounded-lg border border-slate-600/70 bg-[#243044] p-2.5 ${summary.accent}`}>
                  <Icon className="h-5 w-5" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-semibold text-slate-50">{getAgentLabel(provider)}</div>
                  <div className="mt-1 text-xs text-slate-400">{summary.description}</div>
                </div>
              </div>
              {status ? <AgentStatusBadge status={status.status} /> : null}
            </div>
            <ProviderConfigPanel
              provider={provider}
              config={config}
              onModelChange={(value) => onModelChange(provider, value)}
              onArgsChange={(value) => onArgsChange(provider, value)}
            />
          </section>
        );
      })}
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
  const [draft, setDraft] = useState<AppSettings>(() => normalizeSettings(settings));
  const [isSaving, setIsSaving] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [view, setView] = useState<"providers" | "cli">("providers");

  useEffect(() => {
    setDraft(normalizeSettings(settings));
  }, [settings]);

  const statusByType = useMemo(
    () => new Map(statuses.map((status) => [status.agent_type, status])),
    [statuses],
  );

  const planningStatus = draft.planning_agent ? statusByType.get(draft.planning_agent) ?? null : null;
  const executionStatus = draft.execution_agent ? statusByType.get(draft.execution_agent) ?? null : null;
  const hasRequiredDefaults = Boolean(draft.planning_agent && draft.execution_agent);
  const saveDisabled = isSaving || (onboarding && !hasRequiredDefaults);

  function useProviderForBoth(provider: AgentType) {
    setDraft((current) => ({
      ...current,
      planning_agent: provider,
      execution_agent: provider,
    }));
  }

  function updateProviderModel(provider: AgentType, value: string) {
    setDraft((current) => {
      const normalized = normalizeSettings(current);
      const model = value.trim() || null;
      if (provider === "claude_code") {
        return {
          ...normalized,
          agent_configs: {
            ...normalized.agent_configs,
            claude_code: { ...normalized.agent_configs!.claude_code!, model },
          },
        };
      }
      if (provider === "codex") {
        return {
          ...normalized,
          agent_configs: {
            ...normalized.agent_configs,
            codex: { ...normalized.agent_configs!.codex!, model },
          },
        };
      }
      if (provider === "gemini") {
        return {
          ...normalized,
          agent_configs: {
            ...normalized.agent_configs,
            gemini: { ...normalized.agent_configs!.gemini!, model },
          },
        };
      }
      return normalized;
    });
  }

  function updateProviderArgs(provider: AgentType, value: string) {
    setDraft((current) => {
      const normalized = normalizeSettings(current);
      const extra_args = textToArgs(value);
      if (provider === "claude_code") {
        return {
          ...normalized,
          agent_configs: {
            ...normalized.agent_configs,
            claude_code: { ...normalized.agent_configs!.claude_code!, extra_args },
          },
        };
      }
      if (provider === "codex") {
        return {
          ...normalized,
          agent_configs: {
            ...normalized.agent_configs,
            codex: { ...normalized.agent_configs!.codex!, extra_args },
          },
        };
      }
      if (provider === "gemini") {
        return {
          ...normalized,
          agent_configs: {
            ...normalized.agent_configs,
            gemini: { ...normalized.agent_configs!.gemini!, extra_args },
          },
        };
      }
      return normalized;
    });
  }

  async function handleSave() {
    setIsSaving(true);
    try {
      const normalizedDraft = normalizeSettings(draft);
      await onSave({
        ...normalizedDraft,
        agent_setup_seen: settings.agent_setup_seen || hasRequiredDefaults,
        planning_model: normalizedDraft.planning_model?.trim() || null,
        execution_model: normalizedDraft.execution_model?.trim() || null,
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
      <AppModalContent
        titleBarLabel="Agent Bay"
        onClose={onClose}
        closeDisabled={isSaving}
        showTitleBarClose={!forceSetup}
        className="agent-bay-shell flex max-h-[calc(100vh-1.5rem)] max-w-[calc(100vw-1.5rem)] overflow-hidden border-slate-700/70 bg-[#121a2a]/98 p-0 shadow-[0_32px_100px_rgba(2,6,23,0.62)] sm:max-w-4xl"
        shellClassName="agent-bay-scanlines relative max-h-[calc(100vh-1.5rem)]"
      >
        <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_top_left,rgba(14,165,233,0.08),transparent_38%),radial-gradient(circle_at_bottom_right,rgba(168,85,247,0.05),transparent_35%)]" />

        <AppModalHeader
          className="relative"
          title={view === "cli" ? "CLI settings" : onboarding ? "Choose your agents" : "Agent defaults"}
          description={
            view === "cli"
              ? "Set default models and CLI arguments for each provider."
              : "Pick one provider for both roles, or split planning and execution when you need different agents."
          }
          actions={
            <>
              {view === "cli" ? (
                <Button
                  variant="outline"
                  onClick={() => setView("providers")}
                  disabled={isSaving}
                  className="rounded-lg border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                >
                  <ArrowLeft className="h-4 w-4" />
                  Agent defaults
                </Button>
              ) : (
                <>
                  <Button
                    variant="outline"
                    onClick={() => setView("cli")}
                    disabled={isSaving}
                    className="rounded-lg border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                  >
                    <Settings className="h-4 w-4" />
                    CLI settings
                  </Button>
                  <Tooltip content="Checks local CLI availability, login state, and planning/execution support for each provider. This refreshes the readiness badges only; it does not save settings.">
                    <Button
                      variant="outline"
                      onClick={handleRefresh}
                      disabled={isRefreshing || isSaving}
                      className="rounded-lg border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
                    >
                      {isRefreshing ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <RefreshCw className="h-4 w-4" />
                      )}
                      Validate
                    </Button>
                  </Tooltip>
                </>
              )}
            </>
          }
        />

        <AppModalBody className="relative">
          {view === "cli" ? (
            <CliSettingsView
              draft={draft}
              statuses={statusByType}
              onModelChange={updateProviderModel}
              onArgsChange={updateProviderArgs}
            />
          ) : (
            <>
                <div className="grid gap-3 sm:grid-cols-2">
                  <CurrentRoleCard
                    role="planning"
                    focus={focusRole === "planning"}
                    selectedAgent={draft.planning_agent}
                    status={planningStatus}
                  />
                  <CurrentRoleCard
                    role="execution"
                    focus={focusRole === "execution"}
                    selectedAgent={draft.execution_agent}
                    status={executionStatus}
                  />
                </div>

                {onboarding && !hasRequiredDefaults ? (
                  <div className="mt-4 rounded-lg border border-amber-400/20 bg-amber-500/10 px-3 py-2 text-sm text-amber-100">
                    <div className="flex items-start gap-2">
                      <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
                      <div>
                        <div className="font-medium text-amber-50">Choose planning and execution defaults to save.</div>
                        <div className="mt-0.5 text-xs text-amber-100/80">
                          Pick one provider for both roles, or assign planning and execution separately below.
                        </div>
                      </div>
                    </div>
                  </div>
                ) : null}

                <div className="mt-4 space-y-2">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-[11px] uppercase tracking-[0.18em] text-slate-400">Providers</div>
                      <div className="mt-1 text-xs text-slate-400">Use one button for the normal setup, or split roles explicitly.</div>
                    </div>
                  </div>

                  {BUILT_IN_AGENT_TYPES.map((provider) => {
                    const status = statusByType.get(provider) ?? null;
                    return (
                    <ProviderRow
                      key={provider}
                      provider={provider}
                      status={status}
                      selectedForPlanning={draft.planning_agent === provider}
                      selectedForExecution={draft.execution_agent === provider}
                      isSaving={isSaving}
                      onUseBoth={() => useProviderForBoth(provider)}
                      onUsePlanning={() => setDraft((current) => ({ ...current, planning_agent: provider }))}
                      onUseExecution={() => setDraft((current) => ({ ...current, execution_agent: provider }))}
                    />
                  );
                  })}
                </div>
            </>
          )}
        </AppModalBody>

        <AppModalFooter className="relative flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            {onboarding ? (
              <Button
                variant="ghost"
                onClick={handleSkip}
                disabled={isSaving}
                className="rounded-lg text-slate-300 hover:bg-[#243044] hover:text-slate-100"
              >
                Skip for now
              </Button>
            ) : (
              <Button
                variant="ghost"
                onClick={onClose}
                disabled={isSaving}
                className="rounded-lg text-slate-300 hover:bg-[#243044] hover:text-slate-100"
              >
                Cancel
              </Button>
            )}

            <Button
              onClick={handleSave}
              disabled={saveDisabled}
              className="rounded-lg bg-sky-500 text-slate-950 hover:bg-sky-400"
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
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
