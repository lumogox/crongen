import { Minus, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface PathCountControlProps {
  id: string;
  value: number;
  onChange: (value: number) => void;
  description: string;
}

const MIN_PATHS = 1;
const MAX_PATHS = 10;

function clampPathCount(value: number) {
  return Math.min(MAX_PATHS, Math.max(MIN_PATHS, value));
}

export function PathCountControl({ id, value, onChange, description }: PathCountControlProps) {
  const update = (next: number) => onChange(clampPathCount(next));

  return (
    <div className="rounded-xl border border-slate-700/70 bg-[#121a2a] p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <Label htmlFor={id}>Paths to explore</Label>
          <div className="mt-1 text-[11px] leading-snug text-slate-400">
            {description}
          </div>
        </div>
        <div className="flex h-9 shrink-0 items-center overflow-hidden rounded-xl border border-slate-600/80 bg-slate-950/60">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            disabled={value <= MIN_PATHS}
            onClick={() => update(value - 1)}
            className="h-9 w-9 rounded-none border-r border-slate-700/70 text-slate-200 hover:bg-slate-800 disabled:opacity-35"
            aria-label="Decrease paths"
          >
            <Minus className="h-3.5 w-3.5" />
          </Button>
          <Input
            id={id}
            inputMode="numeric"
            value={value}
            onChange={(event) => {
              const next = Number.parseInt(event.target.value, 10);
              if (Number.isFinite(next)) update(next);
            }}
            className="h-9 w-11 rounded-none border-0 bg-transparent px-0 text-center text-sm text-slate-50 shadow-none focus-visible:ring-0"
            aria-label="Number of paths to explore"
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            disabled={value >= MAX_PATHS}
            onClick={() => update(value + 1)}
            className="h-9 w-9 rounded-none border-l border-slate-700/70 text-slate-200 hover:bg-slate-800 disabled:opacity-35"
            aria-label="Increase paths"
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    </div>
  );
}
