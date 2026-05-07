import { AlertTriangle } from "lucide-react";
import type { DecisionNode } from "../types";
import { Dialog } from "@/components/ui/dialog";
import {
  AppModalBody,
  AppModalContent,
  AppModalFooter,
  AppModalHeader,
} from "@/components/ui/app-modal";
import { Button } from "@/components/ui/button";

interface DeleteSessionConfirmProps {
  session: DecisionNode;
  onConfirm: () => void;
  onClose: () => void;
}

export function DeleteSessionConfirm({
  session,
  onConfirm,
  onClose,
}: DeleteSessionConfirmProps) {
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <AppModalContent
        titleBarLabel="Delete session"
        onClose={onClose}
        className="sm:max-w-md"
      >
        <AppModalHeader
          title="Delete session"
          description="This removes the execution flow and cleans up generated worktrees."
        />
        <AppModalBody className="space-y-4">
          <div className="rounded-xl border border-rose-400/25 bg-rose-500/10 p-4">
            <div className="flex gap-3">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-rose-300" />
              <div className="min-w-0">
                <p className="text-sm leading-6 text-slate-200">
                  Delete{" "}
                  <strong className="font-semibold text-slate-50">{session.label}</strong>?
                </p>
                <p className="mt-1 text-xs leading-5 text-slate-400">
                  Any unshipped generated worktrees and temporary crongen branches for this
                  session will be removed. This action cannot be undone.
                </p>
              </div>
            </div>
          </div>
        </AppModalBody>
        <AppModalFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            Delete session
          </Button>
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
