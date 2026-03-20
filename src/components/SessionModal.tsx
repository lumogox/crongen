import { useState } from "react";
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
import { Sparkles, PenLine, Loader2, TriangleAlert, Zap } from "lucide-react";

type PlanComplexity = "linear" | "branching";

interface SessionModalProps {
  onConfirm: (label: string, prompt: string) => void;
  onQuickRun?: (prompt: string) => Promise<void>;
  onGeneratePlan?: (prompt: string, complexity: PlanComplexity) => Promise<void>;
  isGenerating?: boolean;
  onClose: () => void;
}

export function SessionModal({
  onConfirm,
  onQuickRun,
  onGeneratePlan,
  isGenerating,
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

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>New Session</DialogTitle>
          <DialogDescription>
            {mode === "quick"
              ? "Run a single agent directly — fastest for simple tasks."
              : mode === "manual"
                ? "Design the execution tree manually, then run nodes when ready."
                : "Describe the task and Claude will generate an execution plan."}
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
                    ? "Describe the task in detail. Claude will break it down into an execution tree."
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
              disabled={!prompt.trim() || isGenerating}
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
              disabled={!prompt.trim() || isGenerating}
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
