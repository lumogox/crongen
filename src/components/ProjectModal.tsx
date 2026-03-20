import { useState } from "react";
import type { AgentTypeConfig, Project } from "../types";
import { AGENT_TEMPLATES } from "../lib/agent-templates";
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
import { Textarea } from "@/components/ui/textarea";
import {
  Field,
  FieldGroup,
  FieldLabel,
  FieldError,
} from "@/components/ui/field";
import { FolderOpen, PackagePlus, FolderCode } from "lucide-react";

interface ProjectModalProps {
  mode: "create" | "edit";
  project?: Project;
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

function defaultConfig(): AgentTypeConfig {
  return structuredClone(AGENT_TEMPLATES[DEFAULT_AGENT_TYPE].defaultConfig);
}

export function ProjectModal({ mode, project, onSave, onClose }: ProjectModalProps) {
  const [name, setName] = useState(project?.name ?? "");
  const [description, setDescription] = useState(project?.prompt ?? "");
  const [repoPath, setRepoPath] = useState(project?.repo_path ?? "");
  const [projectMode, setProjectMode] = useState<"blank" | "existing">(project?.project_mode ?? "blank");
  const [isActive] = useState(project?.is_active ?? true);

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
  if (!name.trim()) errors.name = "Name is required";
  if (!repoPath.trim()) errors.repoPath = "Project folder is required";
  const canSave = Object.keys(errors).length === 0;

  function handleSave() {
    if (!canSave) return;
    onSave({
      id: project?.id,
      name: name.trim(),
      prompt: description.trim(),
      repoPath: repoPath.trim(),
      agentType: project?.agent_type ?? DEFAULT_AGENT_TYPE,
      typeConfig: project?.type_config ?? defaultConfig(),
      isActive,
      projectMode,
    });
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {mode === "edit" ? "Edit Project" : "New Project"}
          </DialogTitle>
          <DialogDescription>
            {mode === "edit"
              ? "Update your project settings."
              : "Set up a new project to run coding agents on."}
          </DialogDescription>
        </DialogHeader>

        <FieldGroup>
          <Field data-invalid={!!errors.name || undefined}>
            <FieldLabel htmlFor="name">Name</FieldLabel>
            <Input
              id="name"
              placeholder="e.g. My App, Auth Service"
              value={name}
              onChange={(e) => setName(e.target.value)}
              aria-invalid={!!errors.name}
              autoFocus
            />
            <FieldError>{errors.name}</FieldError>
          </Field>

          <Field data-invalid={!!errors.repoPath || undefined}>
            <FieldLabel htmlFor="repoPath">Project Folder</FieldLabel>
            <div className="flex gap-2">
              <Input
                id="repoPath"
                placeholder="/path/to/project"
                value={repoPath}
                onChange={(e) => setRepoPath(e.target.value)}
                aria-invalid={!!errors.repoPath}
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
            <FieldError>{errors.repoPath}</FieldError>
          </Field>

          <Field>
            <FieldLabel>Project type</FieldLabel>
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={() => setProjectMode("blank")}
                className={`flex items-center gap-2.5 rounded-xl border p-3 text-left transition-all ${
                  projectMode === "blank"
                    ? "border-sky-400/40 bg-sky-500/10 ring-1 ring-sky-400/30"
                    : "border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.06]"
                }`}
              >
                <PackagePlus className="h-4 w-4 shrink-0 text-sky-400" />
                <div>
                  <div className="text-sm font-medium text-slate-100">New project</div>
                  <div className="text-[11px] text-slate-500">Scaffold from scratch</div>
                </div>
              </button>
              <button
                type="button"
                onClick={() => setProjectMode("existing")}
                className={`flex items-center gap-2.5 rounded-xl border p-3 text-left transition-all ${
                  projectMode === "existing"
                    ? "border-emerald-400/40 bg-emerald-500/10 ring-1 ring-emerald-400/30"
                    : "border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.06]"
                }`}
              >
                <FolderCode className="h-4 w-4 shrink-0 text-emerald-400" />
                <div>
                  <div className="text-sm font-medium text-slate-100">Existing project</div>
                  <div className="text-[11px] text-slate-500">Add features to existing code</div>
                </div>
              </button>
            </div>
          </Field>

          <Field>
            <FieldLabel htmlFor="description">Description</FieldLabel>
            <Textarea
              id="description"
              placeholder="What is this project about?"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              className="resize-none"
            />
          </Field>
        </FieldGroup>

        <DialogFooter>
          <DialogClose asChild>
            <Button variant="outline">Cancel</Button>
          </DialogClose>
          <Button onClick={handleSave} disabled={!canSave}>
            {mode === "edit" ? "Save" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
