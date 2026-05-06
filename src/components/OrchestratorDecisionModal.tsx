import { useState } from "react";
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
      <AppModalContent titleBarLabel="Orchestrator" onClose={onClose} className="sm:max-w-lg">
        <AppModalHeader title="Decision Required" description={`${decision.label}: ${decision.prompt}`} />
        <AppModalBody className="space-y-2">
          {decision.options.map((option) => (
            <button
              key={option.node_id}
              onClick={() => setSelectedId(option.node_id)}
              className={`w-full rounded-xl border p-3 text-left transition-all ${
                selectedId === option.node_id
                  ? "border-sky-400/40 bg-sky-500/10 ring-1 ring-sky-400/30"
                  : "border-slate-700/70 bg-[#182235] hover:border-slate-500/80 hover:bg-[#243044]"
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
        </AppModalBody>
        <AppModalFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!selectedId}
            onClick={() => selectedId && onSelect(selectedId)}
          >
            Confirm
          </Button>
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
