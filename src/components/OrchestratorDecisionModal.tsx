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
import type { PendingDecision } from "../types";

interface OrchestratorDecisionModalProps {
  decision: PendingDecision;
  onSelect: (nodeId: string) => void;
  onClose: () => void;
}

export function OrchestratorDecisionModal({
  decision,
  onSelect,
  onClose,
}: OrchestratorDecisionModalProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Decision Required</DialogTitle>
          <DialogDescription>
            {decision.label}: {decision.prompt}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2 py-2">
          {decision.options.map((option) => (
            <button
              key={option.node_id}
              onClick={() => setSelectedId(option.node_id)}
              className={`w-full rounded-xl border p-3 text-left transition-all ${
                selectedId === option.node_id
                  ? "border-sky-400/40 bg-sky-500/10 ring-1 ring-sky-400/30"
                  : "border-white/10 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.06]"
              }`}
            >
              <div className="text-sm font-medium text-slate-100">
                {option.label}
              </div>
              <div className="mt-1 text-xs text-slate-400">
                {option.prompt}
              </div>
            </button>
          ))}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!selectedId}
            onClick={() => selectedId && onSelect(selectedId)}
          >
            Confirm
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
