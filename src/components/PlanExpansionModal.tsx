import { useState } from "react";
import { GitFork, Loader2, Sparkles } from "lucide-react";
import type { DecisionNode } from "../types";
import { Dialog } from "@/components/ui/dialog";
import {
  AppModalBody,
  AppModalContent,
  AppModalFooter,
  AppModalHeader,
} from "@/components/ui/app-modal";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

type PlanComplexity = "linear" | "branching";

interface PlanExpansionModalProps {
  parentNode: DecisionNode;
  planningAgentLabel: string;
  isGenerating?: boolean;
  onConfirm: (prompt: string, complexity: PlanComplexity) => Promise<void>;
  onClose: () => void;
}

export function PlanExpansionModal({
  parentNode,
  planningAgentLabel,
  isGenerating,
  onConfirm,
  onClose,
}: PlanExpansionModalProps) {
  const [prompt, setPrompt] = useState("");
  const [complexity, setComplexity] = useState<PlanComplexity>("branching");
  const [error, setError] = useState<string | null>(null);

  async function handleConfirm() {
    if (!prompt.trim()) return;
    setError(null);
    try {
      await onConfirm(prompt.trim(), complexity);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <AppModalContent titleBarLabel="Orchestrator" onClose={onClose} className="sm:max-w-lg">
        <AppModalHeader
          title="Plan from selected node"
          description={`${planningAgentLabel} will generate child steps under "${parentNode.label}".`}
        />
        <AppModalBody className="space-y-4">
          <div className="space-y-2">
            <Label>What should happen next?</Label>
            <Textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="e.g. Split this work into implementation approaches, then compare or synthesize the result."
              rows={5}
              autoFocus
            />
          </div>
          <div className="space-y-2">
            <Label>Plan shape</Label>
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={() => setComplexity("linear")}
                className={`rounded-xl border p-3 text-left transition-all ${
                  complexity === "linear"
                    ? "border-sky-400/40 bg-sky-500/10 ring-1 ring-sky-400/30"
                    : "border-slate-700/70 bg-[#182235] hover:border-slate-500/80 hover:bg-[#243044]"
                }`}
              >
                <div className="flex items-center gap-2 text-xs font-medium text-slate-100">
                  <Sparkles className="h-3.5 w-3.5" />
                  Linear
                </div>
                <div className="mt-1 text-[11px] text-slate-400">A direct chain of work steps</div>
              </button>
              <button
                type="button"
                onClick={() => setComplexity("branching")}
                className={`rounded-xl border p-3 text-left transition-all ${
                  complexity === "branching"
                    ? "border-amber-400/40 bg-amber-500/10 ring-1 ring-amber-400/30"
                    : "border-slate-700/70 bg-[#182235] hover:border-slate-500/80 hover:bg-[#243044]"
                }`}
              >
                <div className="flex items-center gap-2 text-xs font-medium text-slate-100">
                  <GitFork className="h-3.5 w-3.5" />
                  Branching
                </div>
                <div className="mt-1 text-[11px] text-slate-400">Approaches, compare or synthesize, then finish</div>
              </button>
            </div>
          </div>
          {error && (
            <div className="rounded-xl border border-rose-400/20 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
              {error}
            </div>
          )}
        </AppModalBody>
        <AppModalFooter>
          <Button variant="outline" onClick={onClose} disabled={isGenerating}>
            Cancel
          </Button>
          <Button disabled={!prompt.trim() || isGenerating} onClick={handleConfirm}>
            {isGenerating ? (
              <><Loader2 className="mr-2 h-4 w-4 animate-spin" />Planning...</>
            ) : (
              <><Sparkles className="mr-2 h-4 w-4" />Generate steps</>
            )}
          </Button>
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
