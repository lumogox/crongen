import { useState } from "react";
import type { AgentProviderReadiness, AgentRole } from "../types";
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
import { Sparkles, PenLine, Loader2, TriangleAlert, Zap, Settings2, CheckCircle2 } from "lucide-react";

type PlanComplexity = "linear" | "branching";

interface SessionModalProps {
  onConfirm: (label: string, prompt: string) => void;
  onQuickRun?: (prompt: string) => Promise<void>;
  onGeneratePlan?: (prompt: string, complexity: PlanComplexity) => Promise<void>;
  isGenerating?: boolean;
  planningAgentLabel: string;
  executionAgentLabel: string;
  planningStatus: AgentProviderReadiness | null;
  executionStatus: AgentProviderReadiness | null;
  onOpenAgentSetup: (role: AgentRole) => void;
  onClose: () => void;
}

export function SessionModal({
  onConfirm,
  onQuickRun,
  onGeneratePlan,
  isGenerating,
  planningAgentLabel,
  executionAgentLabel,
  planningStatus,
  executionStatus,
  onOpenAgentSetup,
  onClose,
}: SessionModalProps) {
  const [mode, setMode] = useState<"quick" | "manual" | "generate">("quick");
  const [label, setLabel] = useState("");
  const [prompt, setPrompt] = useState("");
  const [complexity, setComplexity] = useState<PlanComplexity>("linear");
  const [genError, setGenError] = useState<string | null>(null);

  async function handleGenerate() {
    if (!onGeneratePlan || !prompt.trim()) return;
    setGenError(null);
    try {
      await onGeneratePlan(prompt.trim(), complexity);
    } catch (e) {
      setGenError(String(e));
    }
  }

  async function handleQuickRun() {
    if (!onQuickRun || !prompt.trim()) return;
    setGenError(null);
    try {
      await onQuickRun(prompt.trim());
    } catch (e) {
      setGenError(String(e));
    }
  }

  const activeAgentLabel = mode === "generate" ? planningAgentLabel : executionAgentLabel;
  const activeStatus = mode === "generate" ? planningStatus : executionStatus;
  const activeRole: AgentRole = mode === "generate" ? "planning" : "execution";
  const agentReady = mode === "manual" ? true : activeStatus?.ready === true;

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>New Session</DialogTitle>
          <DialogDescription>
            {mode === "quick"
              ? `Run a single ${executionAgentLabel} agent directly for the fastest path.`
              : mode === "manual"
                ? "Design the execution tree manually, then run nodes when ready."
                : activeAgentLabel === "Unconfigured"
                  ? "Describe the task and connect a planning agent first."
                  : `Describe the task and ${planningAgentLabel} will generate an execution plan.`}
          </DialogDescription>
        </DialogHeader>

        {/* Mode toggle */}
        <div className="flex items-center rounded-full border border-white/10 bg-white/5 p-0.5">
          {onQuickRun && (
            <button
              onClick={() => { setMode("quick"); setGenError(null); }}
              className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${
                mode === "quick"
                  ? "bg-slate-100 text-slate-950"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <Zap className="h-3 w-3" />
              Quick run
            </button>
          )}
          <button
            onClick={() => { setMode("manual"); setGenError(null); }}
            className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${
              mode === "manual"
                ? "bg-slate-100 text-slate-950"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            <PenLine className="h-3 w-3" />
            Manual
          </button>
          {onGeneratePlan && (
            <button
              onClick={() => setMode("generate")}
              className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${
                mode === "generate"
                  ? "bg-slate-100 text-slate-950"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <Sparkles className="h-3 w-3" />
              Plan
            </button>
          )}
        </div>

        <div className="space-y-4 py-2">
          {mode !== "manual" && (
            activeStatus?.ready ? (
              <div className="flex items-center gap-2 rounded-xl border border-emerald-400/20 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-200">
                <CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
                <span>
                  {activeAgentLabel} is ready for {activeRole}.
                </span>
              </div>
            ) : (
              <div className="rounded-xl border border-amber-400/20 bg-amber-500/10 px-3 py-3 text-xs text-amber-100">
                <div className="flex items-start gap-2">
                  <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="font-medium text-amber-50">
                      {activeAgentLabel === "Unconfigured"
                        ? `No ${activeRole} agent selected`
                        : `${activeAgentLabel} needs setup`}
                    </div>
                    <div className="mt-1 text-amber-100/80">
                      {activeStatus?.detail ?? `Open Agent Bay to choose and validate a ${activeRole} provider.`}
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => onOpenAgentSetup(activeRole)}
                    className="rounded-full border-amber-300/20 bg-black/20 text-amber-50 hover:bg-black/30"
                  >
                    <Settings2 className="h-3.5 w-3.5" />
                    Open setup
                  </Button>
                </div>
              </div>
            )
          )}

          {mode === "manual" && (
            <div className="space-y-2">
              <Label>Name</Label>
              <Input
                value={label}
                onChange={(e) => setLabel(e.target.value)}
                placeholder="e.g. refactor-auth, add-tests"
                autoFocus
              />
            </div>
          )}
          <div className="space-y-2">
            <Label>
              {mode === "quick" ? "What do you need?" : mode === "generate" ? "Task description" : "Task prompt"}
            </Label>
            <Textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder={
                mode === "quick"
                  ? "e.g. Add undo/redo to the calculator using a history stack"
                  : mode === "generate"
                    ? `Describe the task in detail. ${planningAgentLabel} will break it down into an execution tree.`
                    : "What should the agent accomplish?"
              }
              rows={mode === "quick" ? 3 : mode === "generate" ? 6 : 4}
              autoFocus={mode !== "manual"}
            />
          </div>

          {/* Complexity selector for plan generation */}
          {mode === "generate" && (
            <div className="space-y-2">
              <Label>Complexity</Label>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => setComplexity("linear")}
                  className={`rounded-xl border p-2.5 text-left transition-all ${
                    complexity === "linear"
                      ? "border-sky-400/40 bg-sky-500/10 ring-1 ring-sky-400/30"
                      : "border-white/10 bg-white/[0.03] hover:border-white/20"
                  }`}
                >
                  <div className="text-xs font-medium text-slate-100">Linear</div>
                  <div className="text-[11px] text-slate-500">Step-by-step, no branching</div>
                </button>
                <button
                  type="button"
                  onClick={() => setComplexity("branching")}
                  className={`rounded-xl border p-2.5 text-left transition-all ${
                    complexity === "branching"
                      ? "border-violet-400/40 bg-violet-500/10 ring-1 ring-violet-400/30"
                      : "border-white/10 bg-white/[0.03] hover:border-white/20"
                  }`}
                >
                  <div className="text-xs font-medium text-slate-100">Branching</div>
                  <div className="text-[11px] text-slate-500">Compare approaches, then merge</div>
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Inline error display */}
        {genError && (
          <div className="flex items-start gap-2.5 rounded-xl border border-rose-400/20 bg-rose-500/10 px-3.5 py-3 text-sm leading-snug">
            <TriangleAlert className="size-4 shrink-0 mt-0.5 text-rose-400" />
            <div className="min-w-0">
              <div className="font-medium text-rose-200">{mode === "quick" ? "Run failed" : "Plan generation failed"}</div>
              <pre className="mt-1.5 whitespace-pre-wrap break-all text-xs text-rose-300/80 font-mono max-h-40 overflow-y-auto">
                {genError}
              </pre>
            </div>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={isGenerating}>
            Cancel
          </Button>
          {mode === "quick" ? (
            <Button
              disabled={!prompt.trim() || isGenerating || !agentReady}
              onClick={handleQuickRun}
            >
              {isGenerating ? (
                <><Loader2 className="mr-2 h-4 w-4 animate-spin" />Running...</>
              ) : (
                <><Zap className="mr-2 h-4 w-4" />Run</>
              )}
            </Button>
          ) : mode === "manual" ? (
            <Button
              disabled={!label.trim() || !prompt.trim()}
              onClick={() => onConfirm(label.trim(), prompt.trim())}
            >
              Create
            </Button>
          ) : (
            <Button
              disabled={!prompt.trim() || isGenerating || !agentReady}
              onClick={handleGenerate}
            >
              {isGenerating ? (
                <><Loader2 className="mr-2 h-4 w-4 animate-spin" />Generating...</>
              ) : (
                <><Sparkles className="mr-2 h-4 w-4" />{genError ? "Retry" : "Generate"}</>
              )}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
