import { useRef, useState, type ClipboardEvent, type DragEvent } from "react";
import { AlertTriangle, FileText, ImageIcon, Loader2, Paperclip, Trash2 } from "lucide-react";
import type { PromptAttachment, PromptAttachmentInput } from "../types";
import { preparePromptAttachments, removePromptAttachment } from "../lib/tauri-commands";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const MAX_ATTACHMENTS = 10;

interface PromptAttachmentsProps {
  attachments: PromptAttachment[];
  onChange: (attachments: PromptAttachment[]) => void;
  disabled?: boolean;
}

export function PromptAttachments({ attachments, onChange, disabled }: PromptAttachmentsProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [dragging, setDragging] = useState(false);
  const [isPreparing, setIsPreparing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function prepare(inputs: PromptAttachmentInput[]) {
    if (disabled || inputs.length === 0) return;
    setError(null);
    if (attachments.length + inputs.length > MAX_ATTACHMENTS) {
      setError(`Attach at most ${MAX_ATTACHMENTS} context items.`);
      return;
    }

    setIsPreparing(true);
    try {
      const prepared = await preparePromptAttachments(inputs);
      onChange([...attachments, ...prepared]);
    } catch (e) {
      setError(String(e));
    } finally {
      setIsPreparing(false);
    }
  }

  async function chooseFiles() {
    if (disabled) return;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: true,
        title: "Attach context files",
        filters: [
          {
            name: "Context files",
            extensions: ["md", "txt", "json", "csv", "pdf", "docx", "pptx", "xlsx", "html", "htm", "png", "jpg", "jpeg", "gif", "webp"],
          },
        ],
      });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      await prepare(paths.map((path) => {
        const name = path.split(/[\\/]/).pop() || "attachment";
        return {
          name,
          mimeType: guessMime(name),
          kind: inferKind(name, guessMime(name)),
          source: "file",
          path,
        };
      }));
    } catch {
      inputRef.current?.click();
    }
  }

  async function handleFileInput(files: FileList | null) {
    if (!files?.length) return;
    const inputs = await Promise.all(Array.from(files).map(fileToInput));
    await prepare(inputs);
    if (inputRef.current) inputRef.current.value = "";
  }

  async function handleDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    await handleFileInput(event.dataTransfer.files);
  }

  async function handlePaste(event: ClipboardEvent<HTMLDivElement>) {
    if (disabled) return;
    const files = event.clipboardData.files;
    if (files.length > 0) {
      event.preventDefault();
      await handleFileInput(files);
      return;
    }

    const text = event.clipboardData.getData("text/plain");
    if (text.trim()) {
      event.preventDefault();
      await prepare([{
        name: `pasted-context-${attachments.length + 1}.md`,
        mimeType: "text/markdown",
        kind: "text",
        source: "paste",
        text,
      }]);
    }
  }

  async function removeAttachment(attachment: PromptAttachment) {
    onChange(attachments.filter((entry) => entry.id !== attachment.id));
    removePromptAttachment(attachment.storedPath).catch(() => {});
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-xs font-medium text-slate-100">Context</div>
          <div className="text-[11px] text-slate-400">Attach docs, notes, PDFs, or images for the agent to read.</div>
        </div>
        <Button type="button" variant="outline" size="sm" onClick={chooseFiles} disabled={disabled || isPreparing}>
          {isPreparing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Paperclip className="h-3.5 w-3.5" />}
          Attach files
        </Button>
      </div>

      <input
        ref={inputRef}
        type="file"
        className="hidden"
        multiple
        onChange={(event) => handleFileInput(event.target.files)}
      />

      <div
        tabIndex={0}
        onPaste={handlePaste}
        onDragEnter={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={() => setDragging(false)}
        onDrop={handleDrop}
        className={cn(
          "rounded-xl border border-dashed border-slate-700/80 bg-[#0f1726]/80 px-3 py-2 text-[11px] text-slate-400 outline-none transition-colors",
          "focus-visible:border-sky-400/50 focus-visible:ring-2 focus-visible:ring-sky-400/20",
          dragging && "border-sky-400/60 bg-sky-500/10 text-sky-100",
        )}
      >
        Paste notes or files here, or drop files into this area.
      </div>

      {attachments.length > 0 && (
        <div className="space-y-1.5">
          {attachments.map((attachment) => (
            <div
              key={attachment.id}
              className="flex items-center gap-2 rounded-xl border border-slate-700/70 bg-[#182235] px-3 py-2 text-xs"
            >
              {attachment.kind === "image" ? (
                <ImageIcon className="h-3.5 w-3.5 shrink-0 text-sky-300" />
              ) : (
                <FileText className="h-3.5 w-3.5 shrink-0 text-sky-300" />
              )}
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium text-slate-100">{attachment.name}</div>
                <div className="truncate text-[10px] text-slate-400">
                  {formatBytes(attachment.sizeBytes)} · {attachment.kind}
                  {attachment.warning ? ` · ${attachment.warning}` : ""}
                </div>
              </div>
              {attachment.warning && <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-300" />}
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                onClick={() => removeAttachment(attachment)}
                disabled={disabled}
                aria-label={`Remove ${attachment.name}`}
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            </div>
          ))}
        </div>
      )}

      {error && (
        <div className="rounded-lg border border-rose-400/20 bg-rose-500/10 px-3 py-2 text-[11px] text-rose-200">
          {error}
        </div>
      )}
    </div>
  );
}

async function fileToInput(file: File): Promise<PromptAttachmentInput> {
  const mimeType = file.type || guessMime(file.name);
  const kind = inferKind(file.name, mimeType);

  if (kind === "text") {
    return {
      name: file.name,
      mimeType,
      kind,
      source: "file",
      text: await file.text(),
    };
  }

  return {
    name: file.name,
    mimeType,
    kind,
    source: "file",
    dataUrl: await readAsDataUrl(file),
  };
}

function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error ?? new Error("Failed to read file"));
    reader.readAsDataURL(file);
  });
}

function inferKind(name: string, mimeType: string) {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  if (mimeType.startsWith("text/") || ["md", "markdown", "txt", "json", "csv", "xml"].includes(extension)) return "text";
  if (mimeType.startsWith("image/") || ["png", "jpg", "jpeg", "gif", "webp"].includes(extension)) return "image";
  if (extension === "pdf") return "pdf";
  return "document";
}

function guessMime(name: string) {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  switch (extension) {
    case "md":
    case "markdown":
    case "txt":
      return "text/plain";
    case "json":
      return "application/json";
    case "csv":
      return "text/csv";
    case "pdf":
      return "application/pdf";
    case "docx":
      return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    case "pptx":
      return "application/vnd.openxmlformats-officedocument.presentationml.presentation";
    case "xlsx":
      return "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    default:
      return "application/octet-stream";
  }
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
