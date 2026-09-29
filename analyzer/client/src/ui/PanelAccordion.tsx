/**
 * Self-contained stand-in for galaxy's `../PanelAccordion`
 * (PanelAccordion, PanelAccordionItem). Simplified: no radix-ui, plain
 * uncontrolled disclosure per item instead of a shared accordion root.
 */

import { ChevronRightIcon } from "lucide-react";
import type { ReactNode } from "react";
import React, { useState } from "react";

import { cn } from "./utils";

interface PanelAccordionProps {
  children?: ReactNode;
  className?: string;
}

export function PanelAccordion({ children, className }: PanelAccordionProps) {
  return <div className={cn("flex flex-col", className)}>{children}</div>;
}

interface PanelAccordionItemProps {
  value: string;
  label: string;
  actions?: ReactNode;
  children?: ReactNode;
}

export function PanelAccordionItem({ label, actions, children }: PanelAccordionItemProps) {
  const [open, setOpen] = useState(false);

  return (
    <div className="group">
      <header className="mb-[-1px] flex flex-none items-center justify-between border-t border-b py-0.5 pr-1 pl-0.5 group-first:border-t-0">
        <div className="flex items-center gap-0.5">
          <button
            type="button"
            onClick={() => setOpen((prev) => !prev)}
            className={cn(
              "flex h-4 w-4 items-center justify-center text-foreground-muted hover:bg-background-hover hover:text-foreground",
              open && "rotate-90",
            )}
          >
            <ChevronRightIcon className="h-3 w-3" />
          </button>
          <h2 className="m-0 text-[10px] font-medium tracking-widest text-foreground uppercase">
            {label}
          </h2>
        </div>
        <div className="flex items-center gap-0.5">{actions}</div>
      </header>
      {open ? <div className="overflow-hidden">{children}</div> : null}
    </div>
  );
}
