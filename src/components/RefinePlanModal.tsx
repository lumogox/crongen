import { useEffect, useMemo, useState } from "react";
import { Loader2, Sparkles, X } from "lucide-react";
import type { AgentProviderReadiness, AgentType, DecisionNode } from "../types";
import { getAgentLabel } from "../lib/agent-templates";
import { Dialog } from "@/components/ui/dialog";
import {
  AppModalBody,
  AppModalContent,
  AppModalFooter,
  AppModalHeader,
} from "@/components/ui/app-modal";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

const supportedProviders: AgentType[] = ["claude_code", "codex", "gemini"];

const refineLenses = [
  "Tighten prompts",
  "Simplify flow",
  "More technical",
  "More direct",
  "Strengthen checks",
  "Add alternatives",
  "Improve labels",
];

interface RefinePlanModalProps {
  sessionRoot: DecisionNode;
  nodes: DecisionNode[];
  statuses: AgentProviderReadiness[];
  isRefining?: boolean;
  onConfirm: (provider: AgentType, lenses: string[], guidance: string) => Promise<void>;
  onClose: () => void;
}

export function RefinePlanModal({
  sessionRoot,
  nodes,
  statuses,
  isRefining,
  onConfirm,
  onClose,
}: RefinePlanModalProps) {
  const availableProviders = useMemo(
    () =>
      supportedProviders.filter((provider) => {
        const status = statuses.find((item) => item.agent_type === provider);
        return status?.ready && status.supports_planning && !status.coming_soon;
      }),
    [statuses],
  );
  const [provider, setProvider] = useState<AgentType | "">(availableProviders[0] ?? "");
  const [selectedLenses, setSelectedLenses] = useState<string[]>(["Tighten prompts"]);
  const [guidance, setGuidance] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!provider && availableProviders.length > 0) {
      setProvider(availableProviders[0]);
    }
  }, [availableProviders, provider]);

  function toggleLens(lens: string) {
    setSelectedLenses((current) =>
      current.includes(lens)
        ? current.filter((item) => item !== lens)
        : [...current, lens],
    );
  }

  async function handleConfirm() {
    if (!provider) return;
    setError(null);
    try {
      await onConfirm(provider, selectedLenses, guidance.trim());
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <AppModalContent titleBarLabel="Orchestrator" onClose={onClose} className="sm:max-w-xl">
        <AppModalHeader
          title="Refine flow"
          description={`Use a planning agent to rewrite "${sessionRoot.label}" with the current ${nodes.length}-node flow as context.`}
        />
        <AppModalBody className="space-y-5">
          <div className="space-y-2">
            <Label>Refine with</Label>
            <select
              value={provider}
              onChange={(e) => setProvider(e.target.value as AgentType)}
              disabled={availableProviders.length === 0 || isRefining}
              className="h-10 w-full rounded-xl border border-slate-600/70 bg-[#111827] px-3 text-sm text-slate-100 outline-none transition-colors focus:border-sky-400/70 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {availableProviders.length === 0 ? (
                <option value="">No planning agents ready</option>
              ) : (
                availableProviders.map((agent) => (
                  <option key={agent} value={agent}>
                    {getAgentLabel(agent)}
                  </option>
                ))
              )}
            </select>
          </div>

          <div className="space-y-2">
            <div className="flex items-center justify-between gap-3">
              <Label>Lenses</Label>
              <button
                type="button"
                onClick={() => setSelectedLenses([])}
                className="text-xs text-slate-400 transition-colors hover:text-slate-100"
              >
                Clear
              </button>
            </div>
            <div className="flex flex-wrap gap-2">
              {refineLenses.map((lens) => {
                const selected = selectedLenses.includes(lens);
                return (
                  <button
                    key={lens}
                    type="button"
                    onClick={() => toggleLens(lens)}
                    className={`rounded-full border px-3 py-1.5 text-xs font-medium transition-colors ${
                      selected
                        ? "border-sky-400/40 bg-sky-500/15 text-sky-100"
                        : "border-slate-700/70 bg-[#182235] text-slate-300 hover:border-slate-500/80 hover:bg-[#243044]"
                    }`}
                  >
                    {lens}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="space-y-2">
            <Label>Guidance optional</Label>
            <Textarea
              value={guidance}
              onChange={(e) => setGuidance(e.target.value)}
              placeholder="e.g. Steer this toward a smaller MVP, add validation after implementation, and make prompts more specific about performance."
              rows={6}
            />
            <p className="text-xs leading-relaxed text-slate-400">
              The agent receives the full current flow, labels, prompts, node types, statuses, branches, and your guidance.
            </p>
          </div>

          {error && (
            <div className="flex items-start gap-2 rounded-xl border border-rose-400/20 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
              <X className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}
        </AppModalBody>
        <AppModalFooter>
          <Button variant="outline" onClick={onClose} disabled={isRefining}>
            Cancel
          </Button>
          <Button disabled={!provider || isRefining || availableProviders.length === 0} onClick={handleConfirm}>
            {isRefining ? (
              <><Loader2 className="mr-2 h-4 w-4 animate-spin" />Refining...</>
            ) : (
              <><Sparkles className="mr-2 h-4 w-4" />Refine flow</>
            )}
          </Button>
        </AppModalFooter>
      </AppModalContent>
    </Dialog>
  );
}
