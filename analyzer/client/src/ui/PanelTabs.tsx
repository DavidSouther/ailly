/**
 * Self-contained stand-in for galaxy's `../PanelTabs`
 * (PanelTabs, PanelTabsList, PanelTabsTrigger, PanelTabsContent).
 * Simplified: no radix-ui -- a small context-based tab implementation.
 */

import type { ReactNode } from "react";
import React, { createContext, useContext, useMemo, useState } from "react";

import { cn } from "./utils";

interface TabsContextValue {
  value: string;
  setValue: (value: string) => void;
  /**
   * The event a just-switched-to pane should land on, once it mounts. The
   * destination pane clears it when consumed, so a later plain tab click does
   * not replay a stale target.
   */
  target: string | null;
  /** Switches tabs and hands the destination pane something to land on. */
  navigateTo: (tab: string, eventId: string) => void;
  clearTarget: () => void;
}

const TabsContext = createContext<TabsContextValue | null>(null);

/**
 * The active tab and a setter for it, so content rendered inside one tab can
 * hand the user off to another — a summary linking into the transcript, say.
 */
export function usePanelTabs(): TabsContextValue {
  const ctx = useContext(TabsContext);
  if (!ctx) {
    throw new Error("PanelTabs* components must be used within PanelTabs");
  }
  return ctx;
}

/**
 * The tab context when there is one. A lens rendered on its own, outside any
 * tab strip, has no sibling to hand a user to and no tab to be handed from, so
 * the absence is an ordinary case rather than a mistake.
 */
export function usePanelTabsIfPresent(): TabsContextValue | null {
  return useContext(TabsContext);
}

interface PanelTabsProps {
  defaultValue?: string;
  className?: string;
  children?: ReactNode;
}

export function PanelTabs({ defaultValue, className, children }: PanelTabsProps) {
  const [value, setValue] = useState(defaultValue ?? "");
  const [target, setTarget] = useState<string | null>(null);
  const context = useMemo<TabsContextValue>(
    () => ({
      value,
      setValue,
      target,
      navigateTo: (tab, eventId) => {
        setTarget(eventId);
        setValue(tab);
      },
      clearTarget: () => setTarget(null),
    }),
    [value, target],
  );
  return (
    <TabsContext.Provider value={context}>
      <div className={className}>{children}</div>
    </TabsContext.Provider>
  );
}

export function PanelTabsList({ children }: { children?: ReactNode }) {
  return (
    <div role="tablist" className="box-border flex h-8 gap-0">
      {children}
    </div>
  );
}

interface PanelTabsTriggerProps {
  value: string;
  icon?: string;
  label: string;
}

export function PanelTabsTrigger({ value, label }: PanelTabsTriggerProps) {
  const { value: active, setValue } = usePanelTabs();
  const selected = active === value;
  return (
    <button
      type="button"
      role="tab"
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
  const { value: active } = usePanelTabs();
  if (active !== value) return null;
  return (
    <div role="tabpanel" className={className}>
      {children}
    </div>
  );
}
