import { createContext, useContext, useState, type ReactNode } from "react";
import type { VisualNodeType } from "../types/node-types";

interface DndState {
  dragType: VisualNodeType | null;
  setDragType: (type: VisualNodeType | null) => void;
}

const DndCtx = createContext<DndState>({ dragType: null, setDragType: () => {} });

export function DndProvider({ children }: { children: ReactNode }) {
  const [dragType, setDragType] = useState<VisualNodeType | null>(null);
  return (
    <DndCtx.Provider value={{ dragType, setDragType }}>
      {children}
    </DndCtx.Provider>
  );
}

export function useDnd() {
  return useContext(DndCtx);
}
