import type { DecisionNode } from "../types";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

interface DeleteNodeConfirmProps {
  node: DecisionNode;
  onConfirm: () => void;
  onClose: () => void;
}

export function DeleteNodeConfirm({ node, onConfirm, onClose }: DeleteNodeConfirmProps) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Delete Node</DialogTitle>
          <DialogDescription>
            Are you sure you want to delete{" "}
            <strong className="font-semibold text-foreground">{node.label}</strong>?
            This will remove the node, its branch, worktree, and all child nodes.
            This action cannot be undone.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose asChild>
            <Button variant="outline">Cancel</Button>
          </DialogClose>
          <Button variant="destructive" onClick={onConfirm}>
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
