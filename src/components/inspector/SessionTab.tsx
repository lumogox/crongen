import { useEffect, useState } from "react";
import type { DecisionNode, AgentType } from "../../types";
import { usesStructuredSession } from "../../lib/agent-runtime";
import { TerminalView } from "../TerminalView";
import { SdkSessionView } from "../SdkSessionView";

interface SessionTabProps {
  node: DecisionNode;
  agentType: AgentType;
  isActive: boolean;
  manualTerminalSessionId?: string | null;
}

export function SessionTab({
  node,
  agentType,
  isActive,
  manualTerminalSessionId = null,
}: SessionTabProps) {
  const [view, setView] = useState<"agent" | "terminal">(
    manualTerminalSessionId ? "terminal" : "agent",
  );

  useEffect(() => {
    if (manualTerminalSessionId) {
      setView("terminal");
    }
  }, [manualTerminalSessionId]);

  const canShowAgentSession = node.status !== "pending";
  const showManualTerminal = !!manualTerminalSessionId;
  const showToggle = showManualTerminal && canShowAgentSession;

  // Critical: keep terminal DOM mounted but hidden when tab inactive
  // to preserve xterm scroll position and state
  return (
    <div className="h-full" style={{ display: isActive ? "block" : "none" }}>
      {showToggle && (
        <div className="border-b border-white/10 px-4 py-3">
          <div className="flex gap-2">
            <button
              onClick={() => setView("agent")}
              className={`rounded-full px-3 py-1.5 text-xs transition-colors ${
                view === "agent"
                  ? "bg-slate-100 text-slate-950"
                  : "bg-white/5 text-slate-300 hover:bg-white/10"
              }`}
            >
              Agent session
            </button>
            <button
              onClick={() => setView("terminal")}
              className={`rounded-full px-3 py-1.5 text-xs transition-colors ${
                view === "terminal"
                  ? "bg-slate-100 text-slate-950"
                  : "bg-white/5 text-slate-300 hover:bg-white/10"
              }`}
            >
              Agent terminal
            </button>
          </div>
        </div>
      )}
      {showManualTerminal && (!canShowAgentSession || view === "terminal") ? (
        <TerminalView
          sessionId={manualTerminalSessionId}
          status={node.status}
          isInteractive
        />
      ) : usesStructuredSession(agentType) ? (
        <SdkSessionView sessionId={canShowAgentSession ? node.id : null} status={node.status} />
      ) : (
        <TerminalView sessionId={canShowAgentSession ? node.id : null} status={node.status} />
      )}
    </div>
  );
}
