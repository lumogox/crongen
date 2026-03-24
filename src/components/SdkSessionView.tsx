import { useState } from "react";
import { useSdkSession } from "../hooks/useSdkSession";
import type { NodeStatus } from "../types";
import type {
  CodexCommandExecutionItem,
  CodexItemCompletedEvent,
  CodexItemStartedEvent,
  CodexThreadStartedEvent,
  CodexTurnCompletedEvent,
  CodexTurnStartedEvent,
  SdkEvent,
  SdkSystemEvent,
  SdkAssistantEvent,
  SdkStderrEvent,
  SdkToolUseEvent,
  SdkToolResultEvent,
  SdkResultEvent,
  ContentBlock,
  UnknownSdkEvent,
} from "../types/sdk-events";

interface SdkSessionViewProps {
  sessionId: string | null;
  status: NodeStatus;
}

export function SdkSessionView({ sessionId, status }: SdkSessionViewProps) {
  const effectiveSessionId = status === "pending" ? null : sessionId;
  const { events, containerRef } = useSdkSession({
    sessionId: effectiveSessionId,
  });

  const showPlaceholder = !effectiveSessionId;

  return (
    <div className="relative h-full w-full overflow-hidden bg-bg-base">
      <div
        ref={containerRef}
        className="h-full w-full overflow-y-auto px-4 py-3 space-y-2"
      >
        {events.map((event, i) => (
          <EventRenderer key={i} event={event} />
        ))}
        {status === "running" && events.length > 0 && (
          <div className="flex items-center gap-2 py-2">
            <span className="inline-block size-1.5 rounded-full bg-node-running animate-pulse" />
            <span className="text-[11px] text-text-muted">Processing...</span>
          </div>
        )}
      </div>
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

function EventRenderer({ event }: { event: SdkEvent }) {
  switch (event.type) {
    case "system":
      return <SystemEvent event={event} />;
    case "assistant":
      return <AssistantEvent event={event} />;
    case "tool_use":
      return <ToolUseEvent event={event} />;
    case "tool_result":
      return <ToolResultEvent event={event} />;
    case "stderr":
      return <StderrEvent event={event} />;
    case "result":
      return <ResultEvent event={event} />;
    case "thread.started":
      return <CodexThreadStartedEventCard event={event} />;
    case "turn.started":
      return <CodexTurnStartedEventCard event={event} />;
    case "turn.completed":
      return <CodexTurnCompletedEventCard event={event} />;
    case "item.started":
      return <CodexItemEventCard event={event} phase="started" />;
    case "item.completed":
      return <CodexItemEventCard event={event} phase="completed" />;
    default:
      return <UnknownEvent event={event} />;
  }
}

function SystemEvent({ event }: { event: SdkSystemEvent }) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-border-subtle bg-bg-surface px-3 py-2">
      <div className="flex items-center gap-2">
        <span className="text-[10px] font-medium uppercase tracking-wider text-text-muted">
          System
        </span>
        <span className="text-[11px] text-text-secondary">
          {event.subtype}
        </span>
      </div>
      {event.session_id && (
        <div className="mt-1 text-[11px] text-text-muted font-mono truncate">
          Session: {event.session_id}
        </div>
      )}
      {event.tools && event.tools.length > 0 && (
        <div className="mt-1 text-[11px] text-text-muted">
          {event.tools.length} tools available
        </div>
      )}
    </div>
  );
}

function AssistantEvent({ event }: { event: SdkAssistantEvent }) {
  const content = event.message?.content;
  if (!content || content.length === 0) return null;

  return (
    <div className="space-y-1.5">
      {content.map((block, i) => (
        <ContentBlockRenderer key={i} block={block} />
      ))}
    </div>
  );
}

function ContentBlockRenderer({ block }: { block: ContentBlock }) {
  switch (block.type) {
    case "text":
      return (
        <div className="text-[13px] text-text-primary leading-relaxed whitespace-pre-wrap">
          {block.text}
        </div>
      );
    case "tool_use":
      return (
        <CollapsibleCard
          label={block.name}
          badge="Tool Call"
          badgeClass="text-accent"
          defaultOpen={false}
        >
          <pre className="text-[11px] text-text-secondary font-mono whitespace-pre-wrap break-all">
            {formatToolInput(block.name, block.input)}
          </pre>
        </CollapsibleCard>
      );
    case "tool_result":
      return (
        <CollapsibleCard
          label="Tool Result"
          badge={block.is_error ? "Error" : "OK"}
          badgeClass={block.is_error ? "text-node-failed" : "text-node-completed"}
          defaultOpen={false}
        >
          <pre className="text-[11px] text-text-secondary font-mono whitespace-pre-wrap break-all max-h-[300px] overflow-y-auto">
            {block.content}
          </pre>
        </CollapsibleCard>
      );
    default:
      return null;
  }
}

function ToolUseEvent({ event }: { event: SdkToolUseEvent }) {
  return (
    <CollapsibleCard
      label={event.name}
      badge="Tool Call"
      badgeClass="text-accent"
      defaultOpen={false}
    >
      <pre className="text-[11px] text-text-secondary font-mono whitespace-pre-wrap break-all">
        {formatToolInput(event.name, event.input)}
      </pre>
    </CollapsibleCard>
  );
}

function ToolResultEvent({ event }: { event: SdkToolResultEvent }) {
  return (
    <CollapsibleCard
      label={event.name || "Tool Result"}
      badge={event.is_error ? "Error" : "OK"}
      badgeClass={event.is_error ? "text-node-failed" : "text-node-completed"}
      defaultOpen={false}
    >
      <pre className="text-[11px] text-text-secondary font-mono whitespace-pre-wrap break-all max-h-[300px] overflow-y-auto">
        {event.content}
      </pre>
    </CollapsibleCard>
  );
}

function StderrEvent({ event }: { event: SdkStderrEvent }) {
  return (
    <CollapsibleCard
      label="Agent stderr"
      badge="Warning"
      badgeClass="text-node-failed"
      defaultOpen
    >
      <pre className="text-[11px] text-text-secondary font-mono whitespace-pre-wrap break-all max-h-[300px] overflow-y-auto">
        {event.text}
      </pre>
    </CollapsibleCard>
  );
}

function ResultEvent({ event }: { event: SdkResultEvent }) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-border-subtle bg-bg-surface px-3 py-2.5">
      <div className="flex items-center gap-2 mb-2">
        <span className="text-[10px] font-medium uppercase tracking-wider text-node-completed">
          Result
        </span>
        {event.cost_usd != null && (
          <span className="rounded-full bg-bg-elevated px-2 py-0.5 text-[10px] font-mono text-text-muted">
            ${event.cost_usd.toFixed(4)}
          </span>
        )}
        {event.duration_ms != null && (
          <span className="rounded-full bg-bg-elevated px-2 py-0.5 text-[10px] font-mono text-text-muted">
            {(event.duration_ms / 1000).toFixed(1)}s
          </span>
        )}
        {event.is_error && (
          <span className="text-[10px] font-medium text-node-failed">
            ERROR
          </span>
        )}
      </div>
      <div className="text-[13px] text-text-primary leading-relaxed whitespace-pre-wrap">
        {event.result}
      </div>
    </div>
  );
}

function CodexThreadStartedEventCard({
  event,
}: {
  event: CodexThreadStartedEvent;
}) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-border-subtle bg-bg-surface px-3 py-2">
      <div className="flex items-center gap-2">
        <span className="text-[10px] font-medium uppercase tracking-wider text-text-muted">
          Codex
        </span>
        <span className="text-[11px] text-text-secondary">Thread started</span>
      </div>
      <div className="mt-1 text-[11px] font-mono text-text-muted truncate">
        Thread: {event.thread_id}
      </div>
    </div>
  );
}

function CodexTurnStartedEventCard({
  event: _event,
}: {
  event: CodexTurnStartedEvent;
}) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-border-subtle bg-bg-surface px-3 py-2">
      <div className="flex items-center gap-2">
        <span className="text-[10px] font-medium uppercase tracking-wider text-text-muted">
          Codex
        </span>
        <span className="text-[11px] text-text-secondary">Turn started</span>
      </div>
    </div>
  );
}

function CodexTurnCompletedEventCard({
  event,
}: {
  event: CodexTurnCompletedEvent;
}) {
  return (
    <div className="rounded-[var(--radius-sm)] border border-border-subtle bg-bg-surface px-3 py-2">
      <div className="flex items-center gap-2">
        <span className="text-[10px] font-medium uppercase tracking-wider text-node-completed">
          Codex
        </span>
        <span className="text-[11px] text-text-secondary">Turn completed</span>
      </div>
      {event.usage && (
        <div className="mt-2 flex flex-wrap gap-2">
          <UsageChip label="input" value={event.usage.input_tokens} />
          <UsageChip label="cached" value={event.usage.cached_input_tokens} />
          <UsageChip label="output" value={event.usage.output_tokens} />
        </div>
      )}
    </div>
  );
}

function UsageChip({
  label,
  value,
}: {
  label: string;
  value?: number;
}) {
  if (value == null) return null;

  return (
    <span className="rounded-full bg-bg-elevated px-2 py-0.5 text-[10px] font-mono text-text-muted">
      {label}: {value}
    </span>
  );
}

function CodexItemEventCard({
  event,
  phase,
}: {
  event: CodexItemStartedEvent | CodexItemCompletedEvent;
  phase: "started" | "completed";
}) {
  const item = event.item;

  if (isCodexAgentMessageItem(item)) {
    return (
      <div className="space-y-1.5">
        <div className="text-[10px] font-medium uppercase tracking-wider text-text-muted">
          Codex {phase}
        </div>
        <div className="text-[13px] leading-relaxed whitespace-pre-wrap text-text-primary">
          {item.text}
        </div>
      </div>
    );
  }

  if (isCodexCommandExecutionItem(item)) {
    return <CodexCommandExecutionCard item={item} phase={phase} />;
  }

  return <UnknownEvent event={event as UnknownSdkEvent} />;
}

function CodexCommandExecutionCard({
  item,
  phase,
}: {
  item: CodexCommandExecutionItem;
  phase: "started" | "completed";
}) {
  const badge =
    item.status === "completed"
      ? item.exit_code === 0
        ? "OK"
        : "Error"
      : item.status === "in_progress"
        ? "Running"
        : phase;
  const badgeClass =
    item.status === "completed"
      ? item.exit_code === 0
        ? "text-node-completed"
        : "text-node-failed"
      : "text-accent";

  return (
    <CollapsibleCard
      label={item.command}
      badge={badge}
      badgeClass={badgeClass}
      defaultOpen={phase === "completed" && Boolean(item.aggregated_output)}
    >
      <div className="space-y-2">
        <div className="text-[11px] font-mono text-text-secondary whitespace-pre-wrap break-all">
          {item.command}
        </div>
        {item.aggregated_output ? (
          <pre className="max-h-[300px] overflow-y-auto whitespace-pre-wrap break-all text-[11px] font-mono text-text-secondary">
            {item.aggregated_output}
          </pre>
        ) : (
          <div className="text-[11px] text-text-muted">
            {phase === "started" ? "Waiting for command output..." : "No command output."}
          </div>
        )}
        {item.exit_code != null && (
          <div className="text-[10px] font-mono text-text-muted">
            exit code: {item.exit_code}
          </div>
        )}
      </div>
    </CollapsibleCard>
  );
}

function UnknownEvent({ event }: { event: UnknownSdkEvent }) {
  return (
    <CollapsibleCard
      label={event.type || "Unknown event"}
      badge="Raw event"
      badgeClass="text-text-muted"
      defaultOpen={false}
    >
      <pre className="text-[11px] text-text-secondary font-mono whitespace-pre-wrap break-all max-h-[300px] overflow-y-auto">
        {JSON.stringify(event, null, 2)}
      </pre>
    </CollapsibleCard>
  );
}

function isCodexAgentMessageItem(
  item: CodexItemStartedEvent["item"] | CodexItemCompletedEvent["item"],
): item is Extract<CodexItemStartedEvent["item"], { type: "agent_message" }> {
  return item.type === "agent_message" && typeof (item as { text?: unknown }).text === "string";
}

function isCodexCommandExecutionItem(
  item: CodexItemStartedEvent["item"] | CodexItemCompletedEvent["item"],
): item is CodexCommandExecutionItem {
  return item.type === "command_execution"
    && typeof (item as { command?: unknown }).command === "string";
}

// ─── Shared Components ───────────────────────────────────────

function CollapsibleCard({
  label,
  badge,
  badgeClass,
  defaultOpen,
  children,
}: {
  label: string;
  badge: string;
  badgeClass: string;
  defaultOpen: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className="rounded-[var(--radius-sm)] border border-border-subtle bg-bg-surface overflow-hidden">
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-bg-elevated/50 transition-colors"
      >
        <span className="text-[10px] text-text-muted select-none">
          {open ? "▾" : "▸"}
        </span>
        <span className="text-[12px] font-mono text-text-primary truncate">
          {label}
        </span>
        <span className={`ml-auto text-[10px] font-medium ${badgeClass}`}>
          {badge}
        </span>
      </button>
      {open && <div className="border-t border-border-subtle px-3 py-2">{children}</div>}
    </div>
  );
}

// ─── Helpers ─────────────────────────────────────────────────

function formatToolInput(
  toolName: string,
  input: Record<string, unknown>,
): string {
  // Show a compact summary for common tools
  if (toolName === "Read" && input.file_path) {
    return `Read: ${input.file_path}`;
  }
  if (toolName === "Write" && input.file_path) {
    return `Write: ${input.file_path}`;
  }
  if (toolName === "Edit" && input.file_path) {
    return `Edit: ${input.file_path}`;
  }
  if (toolName === "Bash" && input.command) {
    return `$ ${input.command}`;
  }
  if (toolName === "Glob" && input.pattern) {
    return `Glob: ${input.pattern}`;
  }
  if (toolName === "Grep" && input.pattern) {
    return `Grep: ${input.pattern}`;
  }
  return JSON.stringify(input, null, 2);
}
