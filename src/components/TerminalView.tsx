import type { NodeStatus } from "../types";
import { useTerminal } from "../hooks/useTerminal";

interface TerminalViewProps {
  sessionId: string | null;
  status: NodeStatus;
}

export function TerminalView({ sessionId, status }: TerminalViewProps) {
  const isRunning = status === "running";
  // Don't initialize terminal for pending nodes — wait until the PTY session exists.
  // When status transitions pending → running, effectiveSessionId changes null → nodeId,
  // which triggers the useTerminal effect on a fully visible container.
  const effectiveSessionId = status === "pending" ? null : sessionId;
  const { containerRef } = useTerminal({ sessionId: effectiveSessionId, isRunning });

  const showPlaceholder = !effectiveSessionId;

  return (
    <div className="relative h-full w-full overflow-hidden bg-terminal-bg">
      <div ref={containerRef} className="h-full w-full" />
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
