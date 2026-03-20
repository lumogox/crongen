import type { DecisionNode, AgentType } from "../../types";
import { TerminalView } from "../TerminalView";
import { SdkSessionView } from "../SdkSessionView";

interface SessionTabProps {
  node: DecisionNode;
  agentType: AgentType;
  isActive: boolean;
}

export function SessionTab({ node, agentType, isActive }: SessionTabProps) {
  // Critical: keep terminal DOM mounted but hidden when tab inactive
  // to preserve xterm scroll position and state
  return (
    <div className="h-full" style={{ display: isActive ? "block" : "none" }}>
      {agentType === "claude_code" ? (
        <SdkSessionView sessionId={node.id} status={node.status} />
      ) : (
        <TerminalView sessionId={node.id} status={node.status} />
      )}
    </div>
  );
}
