/**
 * Self-contained stand-in for galaxy's `../PanelTabs`
 * (PanelTabs, PanelTabsList, PanelTabsTrigger, PanelTabsContent).
 * Simplified: no radix-ui -- a small context-based tab implementation.
 */

import type { ReactNode } from "react";
import React, { createContext, useContext, useState } from "react";

import { cn } from "./utils";

interface TabsContextValue {
  value: string;
  setValue: (value: string) => void;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabsContext(): TabsContextValue {
  const ctx = useContext(TabsContext);
  if (!ctx) {
    throw new Error("PanelTabs* components must be used within PanelTabs");
  }
  return ctx;
}

interface PanelTabsProps {
  defaultValue?: string;
  className?: string;
  children?: ReactNode;
}

export function PanelTabs({ defaultValue, className, children }: PanelTabsProps) {
  const [value, setValue] = useState(defaultValue ?? "");
  return (
    <TabsContext.Provider value={{ value, setValue }}>
      <div className={className}>{children}</div>
    </TabsContext.Provider>
  );
}

export function PanelTabsList({ children }: { children?: ReactNode }) {
  return <div className="box-border flex h-8 gap-0">{children}</div>;
}

interface PanelTabsTriggerProps {
  value: string;
  icon?: string;
  label: string;
}

export function PanelTabsTrigger({ value, label }: PanelTabsTriggerProps) {
  const { value: active, setValue } = useTabsContext();
  const selected = active === value;
  return (
    <button
      type="button"
      onClick={() => setValue(value)}
      aria-selected={selected}
      className={cn(
        "box-border flex h-8 flex-none items-center gap-2 border-b border-transparent p-2 text-foreground-muted hover:bg-background-hover/50",
        selected && "border-foreground text-foreground",
      )}
    >
      <span className="leading-none font-medium">{label}</span>
    </button>
  );
}

interface PanelTabsContentProps {
  value: string;
  className?: string;
  children?: ReactNode;
}

export function PanelTabsContent({ value, className, children }: PanelTabsContentProps) {
  const { value: active } = useTabsContext();
  if (active !== value) return null;
  return <div className={className}>{children}</div>;
}
