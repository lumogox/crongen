import { useReactFlow } from "@xyflow/react";
import { LayoutGrid } from "lucide-react";
import { Button } from "./ui/button";

interface CanvasToolbarProps {
  title: string;
  subtitle?: string;
  showAutoLayout?: boolean;
}

export function CanvasToolbar({
  title,
  subtitle,
  showAutoLayout = true,
}: CanvasToolbarProps) {
  const { fitView } = useReactFlow();

  return (
    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 rounded-[1.5rem] border border-slate-700/70 bg-[#121a2a] px-4 py-3">
      <div>
        <div className="text-sm font-medium text-slate-100">{title}</div>
        {subtitle && (
          <div className="mt-1 text-xs text-slate-400">{subtitle}</div>
        )}
      </div>

      {showAutoLayout && (
        <Button
          variant="outline"
          onClick={() => fitView({ padding: 0.22, duration: 200 })}
          className="rounded-2xl border-slate-600/70 bg-[#182235] text-slate-100 hover:bg-[#243044]"
        >
          <LayoutGrid className="mr-2 h-4 w-4" />
          Auto-layout
        </Button>
      )}
    </div>
  );
}
