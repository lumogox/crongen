import type { AgentType } from "../types";

export function usesStructuredSession(agentType: AgentType): boolean {
  return agentType === "claude_code" || agentType === "codex";
}

export function usesPtySessionControls(agentType: AgentType): boolean {
  return agentType === "gemini" || agentType === "custom";
}
