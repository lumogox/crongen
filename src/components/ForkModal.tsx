import { useState } from "react";
import type { DecisionNode } from "../types";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";

export type ForkModalMode = "fork" | "task" | "decision" | "agent" | "merge" | "final";

const isExecutable = (mode: ForkModalMode) => mode === "fork" || mode === "task" || mode === "agent";

const modeConfig: Record<
  ForkModalMode,
  { title: string; description: string; confirmLabel: string; textLabel: string; textPlaceholder: string }
> = {
  task: {
    title: "Add Task",
    description: "Create a root task that an agent will execute.",
    confirmLabel: "Create",
    textLabel: "Agent prompt",
    textPlaceholder: "What should the agent do? e.g. 'Set up a Vite project with React and TypeScript'",
  },
  agent: {
    title: "Add Agent",
    description: "Create an agent node that will execute a prompt in its own worktree.",
    confirmLabel: "Create",
    textLabel: "Agent prompt",
    textPlaceholder: "What should this agent do? e.g. 'Implement the auth module using JWT'",
  },
  fork: {
    title: "Add Agent Branch",
    description: "Create a new execution branch with its own worktree.",
    confirmLabel: "Fork",
    textLabel: "Agent prompt",
    textPlaceholder: "What should the agent do differently on this branch?",
  },
  decision: {
    title: "Add Decision Point",
    description: "A branching point where the flow splits into parallel paths. This node does NOT run an agent — it organizes the flow.",
    confirmLabel: "Create",
    textLabel: "Decision description",
    textPlaceholder: "What choice is being made? e.g. 'TypeScript vs JavaScript'",
  },
  merge: {
    title: "Add Review Step",
    description: "A convergence point to compare branches and choose a winner. This node does NOT run an agent.",
    confirmLabel: "Create",
    textLabel: "Review criteria",
    textPlaceholder: "How should branches be compared? e.g. 'Compare test coverage and code quality'",
  },
  final: {
    title: "Add Final Output",
    description: "Mark the approved canonical result. This node does NOT run an agent.",
    confirmLabel: "Create",
    textLabel: "Final notes",
    textPlaceholder: "Any notes about the chosen result...",
  },
};

interface ForkModalProps {
  parentNode: DecisionNode;
  mode?: ForkModalMode;
  onConfirm: (nodeId: string, label: string, prompt: string) => void;
  onClose: () => void;
}

export function ForkModal({
  parentNode,
  mode = "fork",
  onConfirm,
  onClose,
}: ForkModalProps) {
  const config = modeConfig[mode];
  const defaultLabel =
    mode === "agent"
      ? `${parentNode.label}-agent`
      : mode === "fork"
        ? `${parentNode.label}-fork`
        : mode === "decision"
          ? `${parentNode.label}-decision`
          : mode === "merge"
            ? `${parentNode.label}-review`
            : mode === "final"
              ? `${parentNode.label}-final`
              : `${parentNode.label}-task`;

  const [label, setLabel] = useState(defaultLabel);
  const [prompt, setPrompt] = useState("");
  const executable = isExecutable(mode);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{config.title}</DialogTitle>
          <DialogDescription>
            {config.description}
            {" "}From{" "}
            <strong className="font-semibold text-foreground">
              {parentNode.label}
            </strong>
            .
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <Label>Label</Label>
            <Input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="e.g. refactor-auth"
            />
          </div>
          <div className="space-y-2">
            <Label>{config.textLabel}</Label>
            <Textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder={config.textPlaceholder}
              rows={4}
            />
            {executable && (
              <p className="text-[11px] text-slate-500">
                This prompt will be sent to the agent when you click Run.
              </p>
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!prompt.trim()}
            onClick={() => onConfirm(parentNode.id, label.trim(), prompt.trim())}
          >
            {config.confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
