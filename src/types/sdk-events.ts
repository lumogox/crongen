// ─── Claude Code SDK stream-json event types ───────────────────

export type SdkEvent =
  | SdkSystemEvent
  | SdkAssistantEvent
  | SdkToolUseEvent
  | SdkToolResultEvent
  | SdkResultEvent;

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
