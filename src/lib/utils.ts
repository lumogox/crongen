import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import type { DecisionNode } from "../types";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatRelativeTime(unixSeconds: number): string {
  const diff = Math.floor(Date.now() / 1000) - unixSeconds;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 60) return `${minutes}m ${remainingSeconds}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function formatSessionRuntime(
  node: Pick<DecisionNode, "status" | "started_at" | "created_at" | "updated_at">,
): string {
  if (node.status === "pending" && node.started_at == null) {
    return "Queued";
  }

  const start = node.started_at ?? node.created_at;
  const end = node.status === "running" ? Math.floor(Date.now() / 1000) : node.updated_at;
  const elapsed = Math.max(0, end - start);

  if (node.status === "running") {
    return `Running ${formatDuration(elapsed)}`;
  }

  if (node.status === "paused") {
    return `Paused ${formatDuration(elapsed)}`;
  }

  return `Ran ${formatDuration(elapsed)}`;
}
