import type { AppSettings } from "../types";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Brain, Zap } from "lucide-react";

// Claude CLI accepts aliases ("opus", "sonnet", "haiku") that always
// resolve to the latest version, plus full model IDs for pinning.
const MODEL_OPTIONS = [
  { value: "", label: "CLI default" },
  { value: "opus", label: "Opus (latest)" },
  { value: "sonnet", label: "Sonnet (latest)" },
  { value: "haiku", label: "Haiku (latest)" },
  { value: "claude-opus-4-6", label: "Opus 4.6" },
  { value: "claude-sonnet-4-6", label: "Sonnet 4.6" },
  { value: "claude-haiku-4-5-20251001", label: "Haiku 4.5" },
];

interface SettingsModalProps {
  settings: AppSettings;
  onSave: (settings: AppSettings) => void;
  onClose: () => void;
}

function ModelSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full rounded-lg border border-white/10 bg-black/30 px-2.5 py-1.5 text-xs text-slate-200 outline-none focus:border-sky-400/40"
    >
      {MODEL_OPTIONS.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

export function SettingsModal({ settings, onSave, onClose }: SettingsModalProps) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure application preferences.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-2">
          {/* Model selectors */}
          <div className="space-y-3">
            <div className="text-[11px] uppercase tracking-[0.18em] text-slate-500">
              Models
            </div>
            <div className="space-y-2">
              <div className="rounded-xl border border-white/10 bg-black/20 p-3 space-y-2">
                <div className="flex items-center gap-2">
                  <Brain className="h-3.5 w-3.5 text-violet-400" />
                  <span className="text-xs font-medium text-slate-200">Planning</span>
                  <span className="text-[11px] text-slate-500">— task decomposition</span>
                </div>
                <ModelSelect
                  value={settings.planning_model ?? ""}
                  onChange={(v) => onSave({ ...settings, planning_model: v || null })}
                />
              </div>
              <div className="rounded-xl border border-white/10 bg-black/20 p-3 space-y-2">
                <div className="flex items-center gap-2">
                  <Zap className="h-3.5 w-3.5 text-amber-400" />
                  <span className="text-xs font-medium text-slate-200">Execution</span>
                  <span className="text-[11px] text-slate-500">— agent node runs</span>
                </div>
                <ModelSelect
                  value={settings.execution_model ?? ""}
                  onChange={(v) => onSave({ ...settings, execution_model: v || null })}
                />
              </div>
            </div>
          </div>

          {/* Debug mode toggle */}
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="text-sm font-medium text-slate-100">Debug mode</div>
              <div className="text-xs text-slate-400 mt-0.5">
                Show reset controls on completed/failed nodes
              </div>
            </div>
            <button
              onClick={() => onSave({ ...settings, debug_mode: !settings.debug_mode })}
              className={`relative flex h-6 w-11 shrink-0 items-center rounded-full border p-0.5 transition-colors ${
                settings.debug_mode
                  ? "border-sky-400/30 bg-sky-500/20"
                  : "border-white/10 bg-white/5"
              }`}
            >
              <span
                className={`block h-4 w-4 rounded-full transition-all ${
                  settings.debug_mode
                    ? "translate-x-5 bg-sky-400"
                    : "translate-x-0 bg-slate-500"
                }`}
              />
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
