import type { ReactNode } from "react";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type AppModalContentProps = React.ComponentProps<typeof DialogContent> & {
  titleBarLabel: ReactNode;
  onClose?: () => void;
  closeDisabled?: boolean;
  showTitleBarClose?: boolean;
  titleBarActions?: ReactNode;
  shellClassName?: string;
};

function AppModalContent({
  titleBarLabel,
  onClose,
  closeDisabled,
  showTitleBarClose = true,
  titleBarActions,
  shellClassName,
  className,
  children,
  ...props
}: AppModalContentProps) {
  return (
    <DialogContent
      showCloseButton={false}
      className={cn(
        "overflow-hidden border-slate-700/70 bg-[#121a2a]/98 p-0 text-slate-100 shadow-[0_32px_100px_rgba(2,6,23,0.62)]",
        className,
      )}
      {...props}
    >
      <div className={cn("flex max-h-[inherit] min-h-0 w-full flex-col overflow-hidden rounded-lg", shellClassName)}>
        <div className="relative z-10 flex min-h-10 shrink-0 items-center gap-3 border-b border-slate-700/70 pl-5 pr-2 sm:pl-6 sm:pr-2">
          <div className="min-w-0 truncate text-[11px] uppercase tracking-[0.22em] text-sky-300/80">
            {titleBarLabel}
          </div>
          <div className="ml-auto flex shrink-0 items-center gap-2">
            {titleBarActions}
            {showTitleBarClose && onClose && (
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={onClose}
                disabled={closeDisabled}
                aria-label="Close modal"
                className="rounded-lg text-slate-300 hover:bg-[#243044] hover:text-slate-100"
              >
                <X className="h-4 w-4" />
              </Button>
            )}
          </div>
        </div>
        {children}
      </div>
    </DialogContent>
  );
}

function AppModalHeader({
  title,
  description,
  actions,
  children,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <DialogHeader className={cn("border-b border-slate-700/70 px-5 py-4 text-left sm:px-6", className)}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <DialogTitle className="text-xl text-slate-50">{title}</DialogTitle>
          {description && (
            <DialogDescription className="mt-1 max-w-2xl text-sm text-slate-300">
              {description}
            </DialogDescription>
          )}
        </div>
        {actions && <div className="flex shrink-0 flex-wrap justify-end gap-2">{actions}</div>}
      </div>
      {children}
    </DialogHeader>
  );
}

function AppModalBody({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cn("min-h-0 flex-1 overflow-y-auto px-5 py-4 sm:px-6", className)}>
      {children}
    </div>
  );
}

function AppModalFooter({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return (
    <DialogFooter className={cn("border-t border-slate-700/70 bg-[#101827] px-5 py-3 sm:px-6", className)}>
      {children}
    </DialogFooter>
  );
}

export {
  AppModalBody,
  AppModalContent,
  AppModalFooter,
  AppModalHeader,
};
