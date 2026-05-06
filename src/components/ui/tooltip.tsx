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
      <span className="pointer-events-none absolute right-5 top-[calc(100%+0.45rem)] z-[61] h-2.5 w-2.5 rotate-45 border-l border-t border-sky-400/45 bg-[#050916] opacity-0 shadow-[-4px_-4px_14px_rgba(56,189,248,0.08)] transition-opacity group-focus-within/tooltip:opacity-100 group-hover/tooltip:opacity-100" />
      <span
        role="tooltip"
        className={cn(
          "pointer-events-none absolute right-0 top-[calc(100%+0.75rem)] z-[60] w-72 rounded-lg border border-sky-400/45 bg-[#050916] px-3.5 py-3 text-left text-xs leading-5 text-slate-200 opacity-0 shadow-[0_20px_60px_rgba(2,6,23,0.78),0_0_0_1px_rgba(125,211,252,0.10)] transition-opacity group-focus-within/tooltip:opacity-100 group-hover/tooltip:opacity-100",
          contentClassName,
        )}
      >
        {content}
      </span>
    </span>
  );
}

export { Tooltip };
