import { useEffect, useRef, useState } from "react";
import {
  GitMerge,
  GitBranch,
  CheckCircle2,
  AlertTriangle,
  Loader2,
  ArrowRight,
  Check,
  FileText,
  Rocket,
  XCircle,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "./ui/dialog";
import { Button } from "./ui/button";
import type { DecisionNode, MergeResult } from "../types";
import {
  getMergePreview,
  mergeNodeBranch,
  createFeatureBranch,
  markNodeMerged,
} from "../lib/tauri-commands";
import type { MergePreview } from "../lib/tauri-commands";

type MergeStep = "preview" | "merging" | "success" | "conflict" | "error";

interface MergeDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  terminalNode: DecisionNode;
  sessionRootId: string;
  currentBranch: string;
  onComplete: () => void;
}

export function MergeDialog({
  open,
  onOpenChange,
  terminalNode,
  sessionRootId,
  currentBranch,
  onComplete,
}: MergeDialogProps) {
  const [step, setStep] = useState<MergeStep>("preview");
  const [preview, setPreview] = useState<MergePreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(true);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [mergeResult, setMergeResult] = useState<MergeResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [action, setAction] = useState<"merge" | "branch">("merge");
  const [branchName, setBranchName] = useState("");
  const [mergingStepIndex, setMergingStepIndex] = useState(0);
  const [createdBranch, setCreatedBranch] = useState<string | null>(null);
  const timersRef = useRef<ReturnType<typeof setTimeout>[]>([]);

  // Reset state when dialog opens
  useEffect(() => {
    if (!open) return;
    setStep("preview");
    setPreview(null);
    setPreviewLoading(true);
    setPreviewError(null);
    setMergeResult(null);
    setError(null);
    setAction("merge");
    setBranchName("");
    setMergingStepIndex(0);
    setCreatedBranch(null);

    getMergePreview(terminalNode.id)
      .then(setPreview)
      .catch((e) => setPreviewError(String(e)))
      .finally(() => setPreviewLoading(false));
  }, [open, terminalNode.id]);

  // Clean up timers on unmount
  useEffect(() => {
    return () => timersRef.current.forEach(clearTimeout);
  }, []);

  const handleMerge = async () => {
    setStep("merging");
    setMergingStepIndex(0);
    timersRef.current.forEach(clearTimeout);
    timersRef.current = [];

    // Animate stepper while IPC runs
    timersRef.current.push(setTimeout(() => setMergingStepIndex(1), 500));
    timersRef.current.push(setTimeout(() => setMergingStepIndex(2), 1000));

    try {
      const result = await mergeNodeBranch(terminalNode.id);
      timersRef.current.forEach(clearTimeout);
      setMergeResult(result);

      if (result.success) {
        if (result.auto_resolved) {
          setMergingStepIndex(3);
          await new Promise((r) => setTimeout(r, 800));
        }
        await markNodeMerged(sessionRootId).catch(() => {});
        setStep("success");
      } else if (result.conflict_files.length > 0) {
        setStep("conflict");
      } else {
        setError("Merge failed");
        setStep("error");
      }
    } catch (e) {
      timersRef.current.forEach(clearTimeout);
      setError(String(e));
      setStep("error");
    }
  };

  const handleCreateBranch = async () => {
    if (!branchName.trim()) return;
    setStep("merging");
    setMergingStepIndex(0);

    try {
      const created = await createFeatureBranch(
        terminalNode.id,
        branchName.trim(),
      );
      setCreatedBranch(created);
      await markNodeMerged(sessionRootId).catch(() => {});
      setStep("success");
    } catch (e) {
      setError(String(e));
      setStep("error");
    }
  };

  const handleDone = () => {
    onOpenChange(false);
    onComplete();
  };

  const handleBranchFromConflict = () => {
    setStep("preview");
    setAction("branch");
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-lg border-white/10 bg-slate-950 text-slate-100"
        showCloseButton={step !== "merging"}
      >
        {step === "preview" && (
          <PreviewStep
            preview={preview}
            loading={previewLoading}
            error={previewError}
            terminalNode={terminalNode}
            currentBranch={currentBranch}
            action={action}
            onActionChange={setAction}
            branchName={branchName}
            onBranchNameChange={setBranchName}
            onMerge={handleMerge}
            onCreateBranch={handleCreateBranch}
            onCancel={() => onOpenChange(false)}
          />
        )}
        {step === "merging" && (
          <MergingStep
            currentBranch={currentBranch}
            stepIndex={mergingStepIndex}
            isCreatingBranch={action === "branch"}
          />
        )}
        {step === "success" && (
          <SuccessStep
            mergeResult={mergeResult}
            createdBranch={createdBranch}
            currentBranch={currentBranch}
            terminalNode={terminalNode}
            onDone={handleDone}
          />
        )}
        {step === "conflict" && (
          <ConflictStep
            mergeResult={mergeResult}
            onRetry={handleMerge}
            onCreateBranch={handleBranchFromConflict}
            onClose={() => onOpenChange(false)}
          />
        )}
        {step === "error" && (
          <ErrorStep error={error} onClose={() => onOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  );
}

// ─── Preview Step ────────────────────────────────────────────

function PreviewStep({
  preview,
  loading,
  error,
  terminalNode,
  currentBranch,
  action,
  onActionChange,
  branchName,
  onBranchNameChange,
  onMerge,
  onCreateBranch,
  onCancel,
}: {
  preview: MergePreview | null;
  loading: boolean;
  error: string | null;
  terminalNode: DecisionNode;
  currentBranch: string;
  action: "merge" | "branch";
  onActionChange: (action: "merge" | "branch") => void;
  branchName: string;
  onBranchNameChange: (name: string) => void;
  onMerge: () => void;
  onCreateBranch: () => void;
  onCancel: () => void;
}) {
  return (
    <>
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <Rocket className="h-5 w-5 text-violet-400" />
          Ship it
        </DialogTitle>
        <DialogDescription className="text-slate-400">
          Merge your work into {currentBranch}
        </DialogDescription>
      </DialogHeader>

      <div className="space-y-4 py-2">
        {/* Source -> Target */}
        <div className="flex items-center gap-3 rounded-xl border border-white/10 bg-white/[0.03] px-4 py-3">
          <div className="min-w-0 flex-1">
            <div className="text-[10px] uppercase tracking-wider text-slate-500">
              Source
            </div>
            <div className="mt-0.5 truncate font-mono text-sm text-slate-200">
              {terminalNode.branch_name}
            </div>
            {terminalNode.commit_hash && (
              <div className="truncate font-mono text-[11px] text-slate-500">
                {terminalNode.commit_hash.slice(0, 8)}
              </div>
            )}
          </div>
          <ArrowRight className="h-4 w-4 shrink-0 text-slate-500" />
          <div className="min-w-0 flex-1">
            <div className="text-[10px] uppercase tracking-wider text-slate-500">
              Target
            </div>
            <div className="mt-0.5 font-mono text-sm text-slate-200">
              {currentBranch}
            </div>
          </div>
        </div>

        {/* Files changed */}
        {loading ? (
          <div className="flex items-center gap-2 text-xs text-slate-500">
            <Loader2 className="h-3 w-3 animate-spin" />
            Loading preview...
          </div>
        ) : error ? (
          <div className="rounded-xl border border-amber-400/20 bg-amber-500/10 px-3 py-2 text-xs text-amber-200">
            {error}
          </div>
        ) : preview && preview.files_changed.length > 0 ? (
          <div className="rounded-xl border border-white/10 bg-white/[0.03] p-3">
            <div className="mb-2 flex items-center gap-2 text-[10px] uppercase tracking-wider text-slate-500">
              <FileText className="h-3 w-3" />
              {preview.files_changed.length} file
              {preview.files_changed.length === 1 ? "" : "s"} changed
              {preview.commit_count > 0 &&
                ` · ${preview.commit_count} commit${preview.commit_count === 1 ? "" : "s"}`}
            </div>
            <div className="max-h-32 space-y-0.5 overflow-y-auto">
              {preview.files_changed.map((f) => (
                <div
                  key={f}
                  className="truncate font-mono text-[11px] text-slate-400"
                >
                  {f}
                </div>
              ))}
            </div>
          </div>
        ) : preview ? (
          <div className="text-xs text-slate-500">
            No file changes detected
          </div>
        ) : null}

        {/* Action selector */}
        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={() => onActionChange("merge")}
            className={`flex flex-col items-start gap-1 rounded-xl border p-3 text-left transition-colors ${
              action === "merge"
                ? "border-emerald-400/30 bg-emerald-500/10"
                : "border-white/10 bg-white/[0.03] hover:border-white/20"
            }`}
          >
            <GitMerge
              className={`h-4 w-4 ${action === "merge" ? "text-emerald-400" : "text-slate-500"}`}
            />
            <span
              className={`text-xs font-medium ${action === "merge" ? "text-emerald-200" : "text-slate-300"}`}
            >
              Merge to {currentBranch}
            </span>
          </button>
          <button
            onClick={() => onActionChange("branch")}
            className={`flex flex-col items-start gap-1 rounded-xl border p-3 text-left transition-colors ${
              action === "branch"
                ? "border-sky-400/30 bg-sky-500/10"
                : "border-white/10 bg-white/[0.03] hover:border-white/20"
            }`}
          >
            <GitBranch
              className={`h-4 w-4 ${action === "branch" ? "text-sky-400" : "text-slate-500"}`}
            />
            <span
              className={`text-xs font-medium ${action === "branch" ? "text-sky-200" : "text-slate-300"}`}
            >
              Create feature branch
            </span>
          </button>
        </div>

        {/* Branch name input */}
        {action === "branch" && (
          <input
            type="text"
            value={branchName}
            onChange={(e) => onBranchNameChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && branchName.trim()) onCreateBranch();
            }}
            placeholder="feature/my-branch"
            autoFocus
            className="w-full rounded-xl border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-slate-100 outline-none placeholder:text-slate-500 focus:border-sky-400/40"
          />
        )}
      </div>

      <DialogFooter>
        <Button
          variant="outline"
          onClick={onCancel}
          className="border-white/10 text-slate-300 hover:bg-white/5"
        >
          Cancel
        </Button>
        {action === "merge" ? (
          <Button
            onClick={onMerge}
            className="bg-emerald-600 text-white hover:bg-emerald-500"
          >
            <GitMerge className="mr-2 h-4 w-4" />
            Merge to {currentBranch}
          </Button>
        ) : (
          <Button
            onClick={onCreateBranch}
            disabled={!branchName.trim()}
            className="bg-sky-600 text-white hover:bg-sky-500 disabled:opacity-40"
          >
            <GitBranch className="mr-2 h-4 w-4" />
            Create branch
          </Button>
        )}
      </DialogFooter>
    </>
  );
}

// ─── Merging Step ────────────────────────────────────────────

const MERGE_STEP_LABELS = [
  "Auto-committing changes",
  "Checking out {target}",
  "Merging branch...",
  "Resolving conflicts",
];

function MergingStep({
  currentBranch,
  stepIndex,
  isCreatingBranch,
}: {
  currentBranch: string;
  stepIndex: number;
  isCreatingBranch: boolean;
}) {
  const steps = isCreatingBranch
    ? ["Creating branch..."]
    : MERGE_STEP_LABELS.map((s) => s.replace("{target}", currentBranch));

  return (
    <>
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <Loader2 className="h-5 w-5 animate-spin text-sky-400" />
          {isCreatingBranch ? "Creating branch..." : "Merging..."}
        </DialogTitle>
      </DialogHeader>

      <div className="space-y-3 py-4">
        {steps.map((label, i) => {
          const status: "done" | "active" | "pending" =
            i < stepIndex ? "done" : i === stepIndex ? "active" : "pending";
          // Don't show "Resolving conflicts" unless we reach that step
          if (i === 3 && stepIndex < 3) return null;
          return (
            <div key={i} className="flex items-center gap-3">
              <StepIndicator status={status} />
              <span
                className={`text-sm ${
                  status === "done"
                    ? "text-emerald-300"
                    : status === "active"
                      ? "text-sky-200"
                      : "text-slate-500"
                }`}
              >
                {label}
              </span>
            </div>
          );
        })}
      </div>
    </>
  );
}

function StepIndicator({
  status,
}: {
  status: "pending" | "active" | "done";
}) {
  if (status === "done")
    return (
      <div className="flex h-5 w-5 items-center justify-center rounded-full bg-emerald-500/20">
        <Check className="h-3 w-3 text-emerald-400" />
      </div>
    );
  if (status === "active")
    return (
      <div className="flex h-5 w-5 items-center justify-center">
        <Loader2 className="h-4 w-4 animate-spin text-sky-400" />
      </div>
    );
  return <div className="ml-1.5 h-2 w-2 rounded-full bg-slate-600" />;
}

// ─── Success Step ────────────────────────────────────────────

function SuccessStep({
  mergeResult,
  createdBranch,
  currentBranch,
  terminalNode,
  onDone,
}: {
  mergeResult: MergeResult | null;
  createdBranch: string | null;
  currentBranch: string;
  terminalNode: DecisionNode;
  onDone: () => void;
}) {
  return (
    <>
      <DialogHeader>
        <div className="flex flex-col items-center gap-3 pt-2">
          <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-500/20">
            <CheckCircle2 className="h-7 w-7 text-emerald-400" />
          </div>
          <DialogTitle className="text-center">
            {createdBranch ? "Branch created" : "Merged successfully"}
          </DialogTitle>
        </div>
      </DialogHeader>

      <div className="space-y-3 py-2">
        {/* Summary */}
        <div className="rounded-xl border border-white/10 bg-white/[0.03] px-4 py-3 text-center">
          {createdBranch ? (
            <>
              <div className="text-sm text-slate-300">
                <span className="font-mono text-sky-300">{createdBranch}</span>
              </div>
              <div className="mt-1 text-xs text-slate-500">
                Push this branch and open a PR when ready
              </div>
            </>
          ) : (
            <>
              <div className="flex items-center justify-center gap-2 text-sm text-slate-300">
                <span className="max-w-[140px] truncate font-mono text-slate-200">
                  {terminalNode.branch_name}
                </span>
                <ArrowRight className="h-3 w-3 shrink-0 text-slate-500" />
                <span className="font-mono text-emerald-300">
                  {currentBranch}
                </span>
              </div>
              {mergeResult?.merge_commit_hash && (
                <div className="mt-1 font-mono text-[11px] text-slate-500">
                  {mergeResult.merge_commit_hash.slice(0, 8)}
                </div>
              )}
            </>
          )}
        </div>

        {/* Auto-resolution card */}
        {mergeResult?.auto_resolved && mergeResult.resolution_summary && (
          <div className="rounded-xl border border-amber-400/20 bg-amber-500/5 px-4 py-3">
            <div className="mb-1 flex items-center gap-2 text-xs font-medium text-amber-300">
              <AlertTriangle className="h-3 w-3" />
              Conflicts auto-resolved
            </div>
            <div className="text-xs leading-relaxed text-slate-400">
              {mergeResult.resolution_summary}
            </div>
            {mergeResult.conflict_files.length > 0 && (
              <div className="mt-2 space-y-0.5">
                {mergeResult.conflict_files.map((f) => (
                  <div
                    key={f}
                    className="font-mono text-[11px] text-amber-400/70"
                  >
                    {f}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      <DialogFooter>
        <Button
          onClick={onDone}
          className="bg-emerald-600 text-white hover:bg-emerald-500"
        >
          Done
        </Button>
      </DialogFooter>
    </>
  );
}

// ─── Conflict Step ───────────────────────────────────────────

function ConflictStep({
  mergeResult,
  onRetry,
  onCreateBranch,
  onClose,
}: {
  mergeResult: MergeResult | null;
  onRetry: () => void;
  onCreateBranch: () => void;
  onClose: () => void;
}) {
  return (
    <>
      <DialogHeader>
        <div className="flex flex-col items-center gap-3 pt-2">
          <div className="flex h-12 w-12 items-center justify-center rounded-full bg-amber-500/20">
            <AlertTriangle className="h-7 w-7 text-amber-400" />
          </div>
          <DialogTitle className="text-center">Merge conflicts</DialogTitle>
          <DialogDescription className="text-center text-slate-400">
            Auto-resolution was unable to fix these conflicts
          </DialogDescription>
        </div>
      </DialogHeader>

      <div className="py-2">
        {mergeResult && mergeResult.conflict_files.length > 0 && (
          <div className="max-h-48 space-y-0.5 overflow-y-auto rounded-xl border border-white/10 bg-white/[0.03] p-3">
            {mergeResult.conflict_files.map((f) => (
              <div
                key={f}
                className="flex items-center gap-2 font-mono text-[11px] text-rose-300"
              >
                <XCircle className="h-3 w-3 shrink-0 text-rose-400/60" />
                {f}
              </div>
            ))}
          </div>
        )}
      </div>

      <DialogFooter>
        <Button
          variant="outline"
          onClick={onClose}
          className="border-white/10 text-slate-300 hover:bg-white/5"
        >
          Close
        </Button>
        <Button
          variant="outline"
          onClick={onCreateBranch}
          className="border-sky-400/20 bg-sky-500/10 text-sky-200 hover:bg-sky-500/20"
        >
          <GitBranch className="mr-2 h-4 w-4" />
          Create branch instead
        </Button>
        <Button
          onClick={onRetry}
          className="bg-amber-600 text-white hover:bg-amber-500"
        >
          Retry merge
        </Button>
      </DialogFooter>
    </>
  );
}

// ─── Error Step ──────────────────────────────────────────────

function ErrorStep({
  error,
  onClose,
}: {
  error: string | null;
  onClose: () => void;
}) {
  return (
    <>
      <DialogHeader>
        <div className="flex flex-col items-center gap-3 pt-2">
          <div className="flex h-12 w-12 items-center justify-center rounded-full bg-rose-500/20">
            <XCircle className="h-7 w-7 text-rose-400" />
          </div>
          <DialogTitle className="text-center">Merge failed</DialogTitle>
        </div>
      </DialogHeader>

      <div className="py-2">
        <div className="break-words rounded-xl border border-rose-400/20 bg-rose-500/5 px-4 py-3 text-sm text-rose-200">
          {error}
        </div>
      </div>

      <DialogFooter>
        <Button
          onClick={onClose}
          variant="outline"
          className="border-white/10 text-slate-300 hover:bg-white/5"
        >
          Close
        </Button>
      </DialogFooter>
    </>
  );
}
