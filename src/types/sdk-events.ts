// ─── Structured agent session event types ──────────────────────

export type SdkEvent =
  | SdkSystemEvent
  | SdkAssistantEvent
  | SdkToolUseEvent
  | SdkToolResultEvent
  | SdkStderrEvent
  | SdkResultEvent
  | CodexThreadStartedEvent
  | CodexTurnStartedEvent
  | CodexTurnCompletedEvent
  | CodexItemStartedEvent
  | CodexItemCompletedEvent;

export interface SdkSystemEvent {
  type: "system";
  subtype: string;
  session_id?: string;
  tools?: { name: string }[];
  model?: string;
  [key: string]: unknown;
}

export interface SdkAssistantEvent {
  type: "assistant";
  message: {
    content: ContentBlock[];
  };
  session_id: string;
  [key: string]: unknown;
}

export interface SdkToolUseEvent {
  type: "tool_use";
  name: string;
  input: Record<string, unknown>;
  [key: string]: unknown;
}

export interface SdkToolResultEvent {
  type: "tool_result";
  name?: string;
  content: string;
  is_error?: boolean;
  [key: string]: unknown;
}

export interface SdkResultEvent {
  type: "result";
  result: string;
  session_id: string;
  cost_usd?: number;
  duration_ms?: number;
  is_error?: boolean;
  [key: string]: unknown;
}

export interface SdkStderrEvent {
  type: "stderr";
  stream?: "stderr";
  text: string;
  [key: string]: unknown;
}

export interface CodexThreadStartedEvent {
  type: "thread.started";
  thread_id: string;
  [key: string]: unknown;
}

export interface CodexTurnStartedEvent {
  type: "turn.started";
  [key: string]: unknown;
}

export interface CodexTurnCompletedEvent {
  type: "turn.completed";
  usage?: CodexUsage;
  [key: string]: unknown;
}

export interface CodexItemStartedEvent {
  type: "item.started";
  item: CodexItem;
  [key: string]: unknown;
}

export interface CodexItemCompletedEvent {
  type: "item.completed";
  item: CodexItem;
  [key: string]: unknown;
}

export interface CodexUsage {
  input_tokens?: number;
  cached_input_tokens?: number;
  output_tokens?: number;
  [key: string]: unknown;
}

export type CodexItem =
  | CodexAgentMessageItem
  | CodexCommandExecutionItem
  | CodexUnknownItem;

export interface CodexAgentMessageItem {
  id: string;
  type: "agent_message";
  text: string;
  [key: string]: unknown;
}

export interface CodexCommandExecutionItem {
  id: string;
  type: "command_execution";
  command: string;
  aggregated_output?: string;
  exit_code?: number | null;
  status?: string;
  [key: string]: unknown;
}

export interface CodexUnknownItem {
  id?: string;
  type: string;
  [key: string]: unknown;
}

export interface UnknownSdkEvent {
  type: string;
  [key: string]: unknown;
}

export type ContentBlock = TextBlock | ToolUseBlock | ToolResultBlock;

export interface TextBlock {
  type: "text";
  text: string;
}

export interface ToolUseBlock {
  type: "tool_use";
  id: string;
  name: string;
  input: Record<string, unknown>;
}

export interface ToolResultBlock {
  type: "tool_result";
  tool_use_id: string;
  content: string;
  is_error?: boolean;
}
