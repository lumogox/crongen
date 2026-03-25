import type { NodeStatus } from "../types";
import { useTerminal } from "../hooks/useTerminal";

interface TerminalViewProps {
  sessionId: string | null;
  status: NodeStatus;
  isInteractive?: boolean;
}

export function TerminalView({
  sessionId,
  status,
  isInteractive = false,
}: TerminalViewProps) {
  const isRunning = isInteractive || status === "running";
  const effectiveSessionId = sessionId;
  const { containerRef } = useTerminal({ sessionId: effectiveSessionId, isRunning });

  const showPlaceholder = !effectiveSessionId;

  return (
    <div className="relative h-full w-full min-w-0 overflow-hidden bg-terminal-bg">
      <div ref={containerRef} className="h-full w-full min-w-0" />
      {showPlaceholder && (
        <div className="absolute inset-0 flex items-center justify-center">
          <span className="text-[12px] text-text-muted">
            Session not started
          </span>
        </div>
      )}
    </div>
  );
}
