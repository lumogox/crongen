import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface TooltipProps {
  content: ReactNode;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
}

function Tooltip({
  content,
  children,
  className,
  contentClassName,
}: TooltipProps) {
  return (
    <span className={cn("group/tooltip relative inline-flex", className)}>
      {children}
      <span
        role="tooltip"
        className={cn(
          "pointer-events-none absolute right-0 top-[calc(100%+0.5rem)] z-50 w-72 rounded-lg border border-slate-700/70 bg-[#0f1726] px-3 py-2 text-left text-xs leading-5 text-slate-300 opacity-0 shadow-[0_16px_40px_rgba(2,6,23,0.5)] transition-opacity group-focus-within/tooltip:opacity-100 group-hover/tooltip:opacity-100",
          contentClassName,
        )}
      >
        {content}
      </span>
    </span>
  );
}

export { Tooltip };
