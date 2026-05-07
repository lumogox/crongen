export type VisualNodeType = "task" | "decision" | "agent" | "merge" | "synthesis" | "final" | "validation";
export type StructuralNodeType = VisualNodeType;
export type PaletteActionType = StructuralNodeType | "plan";
export type EdgeVariant = "active" | "speculative" | "waiting" | "default";
