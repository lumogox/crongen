import {
  getSmoothStepPath,
  BaseEdge,
  type EdgeProps,
} from "@xyflow/react";
import type { EdgeVariant } from "../types/node-types";

interface ExecutionEdgeData {
  variant: EdgeVariant;
  [key: string]: unknown;
}

const edgeStyles: Record<
  EdgeVariant,
  { stroke: string; strokeWidth: number; strokeDasharray?: string; animated: boolean }
> = {
  active: {
    stroke: "rgba(56,189,248,0.85)",
    strokeWidth: 2.5,
    animated: true,
  },
  speculative: {
    stroke: "rgba(251,191,36,0.9)",
    strokeWidth: 2.5,
    strokeDasharray: "8 8",
    animated: false,
  },
  waiting: {
    stroke: "rgba(100,116,139,0.8)",
    strokeWidth: 2.5,
    strokeDasharray: "5 5",
    animated: false,
  },
  default: {
    stroke: "rgba(56,189,248,0.85)",
    strokeWidth: 2.5,
    animated: false,
  },
};

export function ExecutionEdge({
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  markerEnd,
}: EdgeProps) {
  const variant: EdgeVariant = (data as ExecutionEdgeData)?.variant ?? "default";
  const style = edgeStyles[variant];

  const [edgePath] = getSmoothStepPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  return (
    <BaseEdge
      path={edgePath}
      markerEnd={markerEnd}
      style={{
        stroke: style.stroke,
        strokeWidth: style.strokeWidth,
        strokeDasharray: style.strokeDasharray,
      }}
      className={style.animated ? "react-flow__edge-animated" : ""}
    />
  );
}
