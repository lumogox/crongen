import { useEffect, useState } from "react";
import { AlertTriangle, SquareTerminal, X } from "lucide-react";
import type { DecisionNode, NodeTerminalSession } from "../types";
import { TerminalView } from "./TerminalView";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

function preventDialogDismiss(event: Event) {
  event.preventDefault();
}

interface ConfirmOpenTerminalDialogProps {
  open: boolean;
  node: DecisionNode | null;
  hasExistingSession: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmOpenTerminalDialog({
  open,
  node,
  hasExistingSession,
  onCancel,
  onConfirm,
}: ConfirmOpenTerminalDialogProps) {
  if (!open || !node) return null;

  const locationLabel = node.worktree_path
    ? "This node's worktree"
    : "The merged repo checkout";

  return (
    <Dialog open={open}>
      <DialogContent
        showCloseButton={false}
        className="w-[min(36rem,calc(100vw-2rem))] max-w-[calc(100vw-2rem)] overflow-hidden border-white/10 bg-[#07111f] p-0 text-slate-100 shadow-[0_30px_120px_rgba(2,6,23,0.7)]"
        onPointerDownOutside={preventDialogDismiss}
        onInteractOutside={preventDialogDismiss}
        onEscapeKeyDown={preventDialogDismiss}
      >
        <div className="min-w-0 p-6">
          <DialogHeader className="min-w-0 text-left">
            <div className="flex items-center gap-2 text-sky-200">
              <SquareTerminal className="h-4 w-4" />
              <span className="text-xs uppercase tracking-[0.22em] text-slate-400">
                Agent terminal
              </span>
            </div>
            <DialogTitle className="mt-3 text-xl text-slate-50">
              {hasExistingSession ? "Open existing terminal?" : "Open terminal here?"}
            </DialogTitle>
            <DialogDescription className="mt-2 break-words text-sm leading-6 text-slate-300">
              {hasExistingSession
                ? `This will reopen the live terminal for "${node.label}".`
                : `This will start a fresh interactive agent terminal for "${node.label}".`}
            </DialogDescription>
          </DialogHeader>

          <div className="mt-5 min-w-0 rounded-2xl border border-white/10 bg-black/20 p-4">
            <div className="text-[11px] uppercase tracking-[0.18em] text-slate-500">
              Location
            </div>
            <div className="mt-2 text-sm text-slate-100">{locationLabel}</div>
            {node.worktree_path ? (
              <div
                className="mt-2 break-all font-mono text-xs leading-5 text-slate-400"
                title={node.worktree_path}
              >
                {node.worktree_path}
              </div>
            ) : (
              <div className="mt-2 text-xs text-slate-400">
                If the worktree is gone, we open the agent on the repo branch instead.
              </div>
            )}
          </div>

          <div className="mt-6 flex flex-wrap justify-end gap-2">
            <Button
              variant="outline"
              onClick={onCancel}
              className="rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
            >
              Cancel
            </Button>
            <Button
              onClick={onConfirm}
              className="rounded-2xl bg-sky-500 text-slate-950 hover:bg-sky-400"
            >
              {hasExistingSession ? "Open terminal" : "Start terminal"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

interface NodeTerminalDialogProps {
  open: boolean;
  node: DecisionNode | null;
  terminal: NodeTerminalSession | null;
  onConfirmClose: () => Promise<void> | void;
}

export function NodeTerminalDialog({
  open,
  node,
  terminal,
  onConfirmClose,
}: NodeTerminalDialogProps) {
  const [confirmingClose, setConfirmingClose] = useState(false);
  const [isClosing, setIsClosing] = useState(false);

  useEffect(() => {
    if (open) {
      setConfirmingClose(false);
      setIsClosing(false);
    }
  }, [open, terminal?.session_id]);

  if (!open || !node || !terminal) return null;

  const terminalLabel = terminal.agent_label || "Agent";

  async function handleConfirmClose() {
    setIsClosing(true);
    try {
      await onConfirmClose();
    } finally {
      setIsClosing(false);
    }
  }

  return (
    <Dialog open={open}>
      <DialogContent
        showCloseButton={false}
        className="!w-[calc(100vw-1.5rem)] !max-w-[calc(100vw-1.5rem)] overflow-hidden border-white/10 bg-[#050b16] p-0 text-slate-100 shadow-[0_40px_140px_rgba(2,6,23,0.78)]"
        onPointerDownOutside={preventDialogDismiss}
        onInteractOutside={preventDialogDismiss}
        onEscapeKeyDown={preventDialogDismiss}
      >
        <div className="flex h-[calc(100vh-2rem)] min-h-[620px] flex-col">
          <div className="border-b border-white/10 px-6 py-5">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-xs uppercase tracking-[0.22em] text-slate-500">
                  <SquareTerminal className="h-4 w-4 text-sky-300" />
                  {terminalLabel} terminal
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-3">
                  <div className="truncate text-xl font-semibold text-slate-50">
                    {terminalLabel}
                  </div>
                  {terminal.model && (
                    <span className="rounded-full border border-sky-400/20 bg-sky-500/10 px-2.5 py-1 text-[11px] font-medium text-sky-200">
                      {terminal.model}
                    </span>
                  )}
                </div>
                <div className="mt-2 text-sm text-slate-300">
                  Task: <span className="text-slate-100">{node.label}</span>
                </div>
                <div className="mt-2 break-all font-mono text-xs leading-5 text-slate-400" title={terminal.cwd}>
                  {terminal.cwd}
                </div>
              </div>

              <Button
                variant="outline"
                onClick={() => setConfirmingClose(true)}
                className="rounded-2xl border-rose-400/20 bg-rose-500/10 text-rose-100 hover:bg-rose-500/20"
              >
                <X className="h-4 w-4" />
                Close
              </Button>
            </div>
          </div>

          {confirmingClose && (
            <div className="border-b border-white/10 px-6 py-4">
              <div className="rounded-2xl border border-rose-400/20 bg-rose-500/10 p-4">
                <div className="flex items-start gap-3">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-rose-200" />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-rose-50">
                      Close this terminal?
                    </div>
                    <div className="mt-1 text-sm leading-6 text-rose-100/85">
                      This will end the interactive agent session for this node. We only close it when you confirm here.
                    </div>
                    <div className="mt-3 flex justify-end gap-2">
                      <Button
                        variant="outline"
                        onClick={() => setConfirmingClose(false)}
                        disabled={isClosing}
                        className="rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
                      >
                        Keep open
                      </Button>
                      <Button
                        variant="destructive"
                        onClick={handleConfirmClose}
                        disabled={isClosing}
                        className="rounded-2xl"
                      >
                        {isClosing ? "Closing…" : "Close terminal"}
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          <div className="min-h-0 flex-1 p-4 pt-3">
            <div className="h-full overflow-hidden rounded-[1.35rem] border border-white/10 bg-black/40">
              <TerminalView
                sessionId={terminal.session_id}
                status={node.status}
                isInteractive
              />
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
