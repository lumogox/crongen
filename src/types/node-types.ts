export type VisualNodeType = "task" | "decision" | "agent" | "merge" | "final" | "validation";
export type StructuralNodeType = VisualNodeType;
export type PaletteActionType = StructuralNodeType | "plan";
export type EdgeVariant = "active" | "speculative" | "waiting" | "default";
