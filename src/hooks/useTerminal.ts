import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { listen } from "@tauri-apps/api/event";
import { writePty, resizePty, getSessionOutput } from "../lib/tauri-commands";

interface UseTerminalOptions {
  sessionId: string | null;
  isRunning: boolean;
}

interface UseTerminalReturn {
  containerRef: React.RefObject<HTMLDivElement | null>;
}

interface PtyOutputPayload {
  session_id: string;
  data: string; // base64 encoded
}

/** Decode a base64 string to Uint8Array for proper UTF-8 handling by xterm. */
function base64ToBytes(b64: string): Uint8Array {
  const binaryStr = atob(b64);
  const bytes = new Uint8Array(binaryStr.length);
  for (let i = 0; i < binaryStr.length; i++) {
    bytes[i] = binaryStr.charCodeAt(i);
  }
  return bytes;
}

export function useTerminal({
  sessionId,
  isRunning,
}: UseTerminalOptions): UseTerminalReturn {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // Keep isRunning in a ref so the onData closure always reads the latest value
  const isRunningRef = useRef(isRunning);
  isRunningRef.current = isRunning;

  useEffect(() => {
    if (!sessionId || !containerRef.current) return;

    const container = containerRef.current;
    let disposed = false;
    let unlistenFn: (() => void) | null = null;

    // Create terminal instance
    const term = new Terminal({
      theme: {
        background: "#0E1117",
        foreground: "#E6EDF3",
        cursor: "#58A6FF",
        selectionBackground: "#264F78",
      },
      fontFamily: "'JetBrains Mono', monospace",
      fontSize: 14,
      cursorBlink: true,
      convertEol: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);

    // Initial fit after DOM paint
    requestAnimationFrame(() => fitAddon.fit());

    termRef.current = term;
    fitAddonRef.current = fitAddon;

    const sid = sessionId;

    // ─── Output pipeline: buffer first, then live events ─────
    // Fetch buffered output, write it, THEN register the live listener.
    // This prevents the duplication that occurs when both run simultaneously.
    getSessionOutput(sid)
      .then((buffered) => {
        if (disposed) return;
        if (buffered) {
          term.write(base64ToBytes(buffered));
        }
      })
      .catch(() => {})
      .finally(() => {
        if (disposed) return;
        listen<PtyOutputPayload>("pty_output", (event) => {
          if (event.payload.session_id !== sid) return;
          term.write(base64ToBytes(event.payload.data));
        }).then((fn) => {
          if (disposed) {
            fn(); // already cleaned up — unlisten immediately
          } else {
            unlistenFn = fn;
          }
        });
      });

    // ─── Keyboard input → PTY ───────────────────────────────
    const dataDisposable = term.onData((data) => {
      if (isRunningRef.current) {
        writePty(sid, data).catch(() => {});
      }
    });

    // ─── Resize observer ────────────────────────────────────
    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        fitAddon.fit();
        resizePty(sid, term.rows, term.cols).catch(() => {});
      });
    });
    observer.observe(container);

    // ─── Cleanup ────────────────────────────────────────────
    return () => {
      disposed = true;
      observer.disconnect();
      dataDisposable.dispose();
      if (unlistenFn) unlistenFn();
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
    };
  }, [sessionId]); // intentionally omit isRunning — onData reads from isRunningRef

  return { containerRef };
}
