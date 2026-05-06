import type { Project } from "../types";
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

interface DeleteConfirmProps {
  project: Project;
  onConfirm: () => void;
  onClose: () => void;
}

export function DeleteConfirm({ project, onConfirm, onClose }: DeleteConfirmProps) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <AppModalContent titleBarLabel="Confirm" onClose={onClose} className="sm:max-w-sm">
        <AppModalHeader title="Delete Project" />
        <AppModalBody>
          <p className="text-sm leading-6 text-slate-300">
            Are you sure you want to delete{" "}
            <strong className="font-semibold text-foreground">{project.name}</strong>?
            This will remove the project and all its session history. This
            action cannot be undone.
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
