import { useState } from "react";
import type { DecisionNode } from "../types";
import {
  Dialog,
} from "@/components/ui/dialog";
import {
  AppModalBody,
  AppModalContent,
  AppModalFooter,
  AppModalHeader,
} from "@/components/ui/app-modal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import type { StructuralNodeType } from "../types/node-types";

export type ForkModalMode = "fork" | StructuralNodeType;

const promptRunsWithAgent = (mode: ForkModalMode) => mode !== "decision" && mode !== "validation";

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
    title: "Add Work Step",
    description: "Create an executable step that runs the project's execution provider in its own worktree.",
    confirmLabel: "Create",
    textLabel: "Work prompt",
    textPlaceholder: "What should this step do? e.g. 'Implement the auth module using JWT'",
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
    title: "Add Compare Step",
    description: "A convergence step that compares sibling branches and picks the single best result.",
    confirmLabel: "Create",
    textLabel: "Comparison criteria",
    textPlaceholder: "How should the winning branch be chosen? e.g. 'Prioritize test coverage and maintainability'",
  },
  synthesis: {
    title: "Add Synthesize Step",
    description: "A convergence step that combines useful parts from sibling branches into one better result.",
    confirmLabel: "Create",
    textLabel: "Synthesis criteria",
    textPlaceholder: "What should be combined? e.g. 'Keep the safer architecture and the stronger UI polish'",
  },
  final: {
    title: "Add Finish Step",
    description: "Polish and integrate the selected result after comparison.",
    confirmLabel: "Create",
    textLabel: "Finish prompt",
    textPlaceholder: "What final polish is needed? e.g. 'Update tests, docs, and UI copy'",
  },
  validation: {
    title: "Add Validation Step",
    description: "Run the repository's detected local checks after this point in the flow.",
    confirmLabel: "Create",
    textLabel: "Validation notes",
    textPlaceholder: "What should be verified? e.g. 'Run build and tests before shipping'",
  },
};

interface ForkModalProps {
  parentNode: DecisionNode;
  mode?: ForkModalMode;
  onConfirm: (nodeId: string, label: string, prompt: string) => void;
  onClose: () => void;
}

function defaultNodeLabel(parentLabel: string, mode: ForkModalMode): string {
  switch (mode) {
    case "agent":
      return `${parentLabel}-agent`;
    case "fork":
      return `${parentLabel}-fork`;
    case "decision":
      return `${parentLabel}-decision`;
    case "merge":
      return `${parentLabel}-compare`;
    case "synthesis":
      return `${parentLabel}-synthesize`;
    case "final":
      return `${parentLabel}-final`;
    case "validation":
      return `${parentLabel}-validate`;
    default:
      return `${parentLabel}-task`;
  }
}

export function ForkModal({
  parentNode,
  mode = "fork",
  onConfirm,
  onClose,
}: ForkModalProps) {
  const config = modeConfig[mode];
  const defaultLabel = defaultNodeLabel(parentNode.label, mode);

  const [label, setLabel] = useState(defaultLabel);
  const [prompt, setPrompt] = useState("");
  const usesAgentPrompt = promptRunsWithAgent(mode);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <AppModalContent titleBarLabel="Execution graph" onClose={onClose} className="sm:max-w-md">
        <AppModalHeader
          title={config.title}
          description={
            <>
            {config.description}
            {" "}From{" "}
            <strong className="font-semibold text-foreground">
              {parentNode.label}
            </strong>
            .
            </>
          }
        />
        <AppModalBody className="space-y-4">
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
            {usesAgentPrompt && (
              <p className="text-[11px] text-slate-400">
                This prompt will be sent to the agent when you click Run.
              </p>
            )}
            {mode === "validation" && (
              <p className="text-[11px] text-slate-400">
                Validation runs detected local checks; these notes are stored for context.
              </p>
            )}
          </div>
        </AppModalBody>
        <AppModalFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!prompt.trim()}
            onClick={() => onConfirm(parentNode.id, label.trim(), prompt.trim())}
          >
            {config.confirmLabel}
          </Button>
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
