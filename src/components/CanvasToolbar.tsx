import { useReactFlow } from "@xyflow/react";
import { LayoutGrid } from "lucide-react";
import { Button } from "./ui/button";

interface CanvasToolbarProps {
  title: string;
  subtitle?: string;
}

export function CanvasToolbar({
  title,
  subtitle,
}: CanvasToolbarProps) {
  const { fitView } = useReactFlow();

  return (
    <div className="shrink-0 flex flex-wrap items-center justify-between gap-3 rounded-[1.5rem] border border-white/10 bg-white/[0.03] px-4 py-3">
      <div>
        <div className="text-sm font-medium text-slate-100">{title}</div>
        {subtitle && (
          <div className="mt-1 text-xs text-slate-500">{subtitle}</div>
        )}
      </div>

      <Button
        variant="outline"
        onClick={() => fitView({ padding: 0.22, duration: 200 })}
        className="rounded-2xl border-white/10 bg-white/5 text-slate-100 hover:bg-white/10"
      >
        <LayoutGrid className="mr-2 h-4 w-4" />
        Auto-layout
      </Button>
    </div>
  );
}
