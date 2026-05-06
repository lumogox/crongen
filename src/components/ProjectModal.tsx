import { useState } from "react";
import type { AgentType, AgentTypeConfig, Project } from "../types";
import { AGENT_TEMPLATES, getAgentLabel } from "../lib/agent-templates";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Field,
  FieldGroup,
  FieldLabel,
  FieldError,
} from "@/components/ui/field";
import { FolderOpen, PackagePlus, FolderCode, Bot, Sparkles, BrainCircuit, Orbit, Check } from "lucide-react";

interface ProjectModalProps {
  mode: "create" | "edit";
  project?: Project;
  defaultExecutionAgent: AgentType | null | undefined;
  onSave: (params: {
    id?: string;
    name: string;
    prompt: string;
    repoPath: string;
    agentType: string;
    typeConfig: AgentTypeConfig;
    isActive: boolean;
    projectMode?: string;
  }) => void;
  onClose: () => void;
}

const DEFAULT_AGENT_TYPE = "claude_code";
type ProjectAgentChoice = "default" | "claude_code" | "codex" | "gemini";

function defaultConfig(): AgentTypeConfig {
  return structuredClone(AGENT_TEMPLATES[DEFAULT_AGENT_TYPE].defaultConfig);
}

function choiceClass(selected: boolean, tone = "sky") {
  const active =
    tone === "emerald"
      ? "border-emerald-400/45 bg-emerald-500/10 ring-1 ring-emerald-400/25"
      : tone === "violet"
        ? "border-violet-400/45 bg-violet-500/10 ring-1 ring-violet-400/25"
        : tone === "amber"
          ? "border-amber-400/45 bg-amber-500/10 ring-1 ring-amber-400/25"
          : "border-sky-400/45 bg-sky-500/10 ring-1 ring-sky-400/25";

  return `rounded-xl border p-3 text-left transition-all ${
    selected
      ? active
      : "border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.06]"
  }`;
}

function resolveInitialAgentChoice(
  project: Project | undefined,
  _defaultExecutionAgent: AgentType | null | undefined,
): ProjectAgentChoice {
  if (!project) return "default";
  if (project.agent_type === "codex") return "codex";
  if (project.agent_type === "gemini") return "gemini";
  return "claude_code";
}

export function ProjectModal({ mode, project, defaultExecutionAgent, onSave, onClose }: ProjectModalProps) {
  const [name, setName] = useState(project?.name ?? "");
  const [repoPath, setRepoPath] = useState(project?.repo_path ?? "");
  const [projectMode, setProjectMode] = useState<"blank" | "existing">(project?.project_mode ?? "blank");
  const [isActive] = useState(project?.is_active ?? true);
  const [agentChoice, setAgentChoice] = useState<ProjectAgentChoice>(
    resolveInitialAgentChoice(project, defaultExecutionAgent),
  );
  const [submitAttempted, setSubmitAttempted] = useState(false);
  const isExistingProject = projectMode === "existing";
  const dialogTitle = mode === "edit" ? "Edit project folder" : "Add project folder";
  const folderLabel = isExistingProject ? "Existing code folder" : "Destination folder";

  async function handleBrowseFolder() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, title: "Select Project Folder" });
      if (selected) setRepoPath(selected as string);
    } catch {
      // Not running in Tauri context
    }
  }

  const errors: Record<string, string> = {};
  if (!name.trim()) errors.name = "Add a project name";
  if (!repoPath.trim()) errors.repoPath = "Choose a folder";
  const resolvedAgentType =
    agentChoice === "default"
      ? defaultExecutionAgent ?? null
      : agentChoice;
  if (!resolvedAgentType) {
    errors.agentType = "Choose Claude Code, Codex, Gemini, or configure a default execution agent in Agent Bay";
  }
  const canSave = Object.keys(errors).length === 0;
  const showValidationErrors = submitAttempted && !canSave;

  function handleSave() {
    setSubmitAttempted(true);
    if (!canSave || !resolvedAgentType) return;
    const nextConfig =
      project?.agent_type === resolvedAgentType
        ? project.type_config
        : structuredClone(AGENT_TEMPLATES[resolvedAgentType].defaultConfig);

    onSave({
      id: project?.id,
      name: name.trim(),
      prompt: project?.prompt ?? "",
      repoPath: repoPath.trim(),
      agentType: resolvedAgentType,
      typeConfig: nextConfig ?? defaultConfig(),
      isActive,
      projectMode,
    });
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="max-h-[calc(100vh-4rem)] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{dialogTitle}</DialogTitle>
          <DialogDescription>
            {mode === "edit"
              ? "Update how agents work with this folder."
              : "Start from an empty folder or connect agents to code you already have."}
          </DialogDescription>
        </DialogHeader>

        <FieldGroup>
          <Field>
            <FieldLabel>What are you starting from?</FieldLabel>
            <div className="grid gap-2 sm:grid-cols-2">
              <button
                type="button"
                onClick={() => setProjectMode("blank")}
                className={choiceClass(projectMode === "blank")}
              >
                <div className="flex items-start gap-3">
                  <PackagePlus className="mt-0.5 h-4 w-4 shrink-0 text-sky-300" />
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-slate-100">Start new</div>
                    <div className="mt-0.5 text-[11px] leading-4 text-slate-500">
                      Use an empty folder and let agents scaffold the project.
                    </div>
                  </div>
                </div>
              </button>
              <button
                type="button"
                onClick={() => setProjectMode("existing")}
                className={choiceClass(projectMode === "existing", "emerald")}
              >
                <div className="flex items-start gap-3">
                  <FolderCode className="mt-0.5 h-4 w-4 shrink-0 text-emerald-300" />
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-slate-100">Open existing</div>
                    <div className="mt-0.5 text-[11px] leading-4 text-slate-500">
                      Use a folder that already contains code.
                    </div>
                  </div>
                </div>
              </button>
            </div>
          </Field>

          <Field data-invalid={(showValidationErrors && !!errors.name) || undefined}>
            <FieldLabel htmlFor="name">Display name</FieldLabel>
            <Input
              id="name"
              placeholder="e.g. Auth Service"
              value={name}
              onChange={(e) => setName(e.target.value)}
              aria-invalid={showValidationErrors && !!errors.name}
              autoFocus
            />
            <FieldError>{showValidationErrors ? errors.name : undefined}</FieldError>
          </Field>

          <Field data-invalid={(showValidationErrors && !!errors.repoPath) || undefined}>
            <FieldLabel htmlFor="repoPath">{folderLabel}</FieldLabel>
            <div className="flex gap-2">
              <Input
                id="repoPath"
                placeholder={isExistingProject ? "/path/to/existing/code" : "/path/to/new/project"}
                value={repoPath}
                onChange={(e) => setRepoPath(e.target.value)}
                aria-invalid={showValidationErrors && !!errors.repoPath}
              />
              <Button
                type="button"
                variant="outline"
                size="icon"
                onClick={handleBrowseFolder}
                className="shrink-0"
              >
                <FolderOpen className="size-4" />
              </Button>
            </div>
            <FieldError>{showValidationErrors ? errors.repoPath : undefined}</FieldError>
          </Field>

          <Field data-invalid={(showValidationErrors && !!errors.agentType) || undefined}>
            <FieldLabel>Execution agent</FieldLabel>
            <div className="grid gap-2">
              <button
                type="button"
                onClick={() => setAgentChoice("default")}
                className={choiceClass(agentChoice === "default")}
              >
                <div className="flex items-center gap-2.5">
                  <Sparkles className="h-4 w-4 shrink-0 text-sky-300" />
                  <div>
                    <div className="text-sm font-medium text-slate-100">
                      Use Agent Bay default
                    </div>
                    <div className="text-[11px] text-slate-500">
                      {defaultExecutionAgent
                        ? `${getAgentLabel(defaultExecutionAgent)} will be copied into this project.`
                        : "Configure a default in Agent Bay or choose an agent below."}
                    </div>
                  </div>
                </div>
                {agentChoice === "default" && <Check className="h-4 w-4 shrink-0 text-sky-300" />}
              </button>

              <div className="grid gap-2 sm:grid-cols-3">
                <button
                  type="button"
                  onClick={() => setAgentChoice("claude_code")}
                  className={choiceClass(agentChoice === "claude_code", "violet")}
                >
                  <div className="flex min-w-0 items-start gap-2.5">
                    <BrainCircuit className="mt-0.5 h-4 w-4 shrink-0 text-violet-300" />
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-slate-100">Claude Code</div>
                      <div className="text-[11px] leading-4 text-slate-500">Claude CLI</div>
                    </div>
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() => setAgentChoice("codex")}
                  className={choiceClass(agentChoice === "codex", "emerald")}
                >
                  <div className="flex min-w-0 items-start gap-2.5">
                    <Bot className="mt-0.5 h-4 w-4 shrink-0 text-emerald-300" />
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-slate-100">Codex</div>
                      <div className="text-[11px] leading-4 text-slate-500">OpenAI CLI</div>
                    </div>
                  </div>
                </button>
                <button
                  type="button"
                  onClick={() => setAgentChoice("gemini")}
                  className={choiceClass(agentChoice === "gemini", "amber")}
                >
                  <div className="flex min-w-0 items-start gap-2.5">
                    <Orbit className="mt-0.5 h-4 w-4 shrink-0 text-amber-200" />
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-slate-100">Gemini</div>
                      <div className="text-[11px] leading-4 text-slate-500">Gemini CLI</div>
                    </div>
                  </div>
                </button>
              </div>
            </div>
            <FieldError>{showValidationErrors ? errors.agentType : undefined}</FieldError>
          </Field>
        </FieldGroup>

        <DialogFooter>
          <DialogClose asChild>
            <Button variant="outline">Cancel</Button>
          </DialogClose>
          <Button onClick={handleSave}>
            {mode === "edit" ? "Save changes" : "Add folder"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
