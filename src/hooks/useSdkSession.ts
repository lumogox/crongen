import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getSdkSessionOutput } from "../lib/tauri-commands";
import type { SdkEvent } from "../types/sdk-events";

interface UseSdkSessionOptions {
  sessionId: string | null;
}

interface SdkOutputPayload {
  session_id: string;
  data: string; // raw JSON line
}

function parseSdkLine(line: string): SdkEvent | null {
  try {
    return JSON.parse(line) as SdkEvent;
  } catch {
    return null;
  }
}

export function useSdkSession({ sessionId }: UseSdkSessionOptions) {
  const [events, setEvents] = useState<SdkEvent[]>([]);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!sessionId) {
      setEvents([]);
      return;
    }

    let disposed = false;
    let unlistenFn: (() => void) | null = null;

    // Fetch buffered output first, then subscribe to live events
    getSdkSessionOutput(sessionId)
      .then((lines) => {
        if (disposed) return;
        const parsed = lines
          .map(parseSdkLine)
          .filter((e): e is SdkEvent => e !== null);
        setEvents(parsed);
      })
      .catch(() => {})
      .finally(() => {
        if (disposed) return;
        const sid = sessionId;
        listen<SdkOutputPayload>("sdk_output", (event) => {
          if (event.payload.session_id !== sid) return;
          const parsed = parseSdkLine(event.payload.data);
          if (parsed) {
            setEvents((prev) => [...prev, parsed]);
          }
        }).then((fn) => {
          if (disposed) {
            fn();
          } else {
            unlistenFn = fn;
          }
        });
      });

    return () => {
      disposed = true;
      if (unlistenFn) unlistenFn();
    };
  }, [sessionId]);

  // Auto-scroll to bottom on new events
  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [events]);

  return { events, containerRef };
}
