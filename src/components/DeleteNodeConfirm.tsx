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

interface DeleteNodeConfirmProps {
  node: DecisionNode;
  onConfirm: () => void;
  onClose: () => void;
}

export function DeleteNodeConfirm({ node, onConfirm, onClose }: DeleteNodeConfirmProps) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <AppModalContent titleBarLabel="Confirm" onClose={onClose} className="sm:max-w-sm">
        <AppModalHeader title="Delete Node" />
        <AppModalBody>
          <p className="text-sm leading-6 text-slate-300">
            Are you sure you want to delete{" "}
            <strong className="font-semibold text-foreground">{node.label}</strong>?
            This will remove the node, its branch, worktree, and all child nodes.
            This action cannot be undone.
          </p>
        </AppModalBody>
        <AppModalFooter>
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button variant="destructive" onClick={onConfirm}>
            Delete
          </Button>
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
