/**
 * Self-contained stand-in for galaxy's `../CollapsibleResizePanel`.
 * Simplified: no drag-to-resize handle, no container ResizeObserver-driven
 * auto-collapse -- just a fixed-width panel that renders `null` when
 * collapsed, matching the prop surface the stories actually use.
 */

import type { ReactNode } from "react";
import React from "react";

import { cn } from "./utils";

interface CollapsibleResizePanelProps {
  collapsed: boolean;
  onResizeCollapse?: () => void;
  width: number;
  onWidthChange: (width: number) => void;
  defaultWidth?: number;
  minExpandedWidth?: number;
  maxWidth?: string | number;
  side?: "left" | "right";
  containerRef?: React.RefObject<HTMLElement | null>;
  className?: string;
  children: ReactNode;
}

export function CollapsibleResizePanel({
  collapsed,
  width,
  defaultWidth = 280,
  className,
  children,
}: CollapsibleResizePanelProps) {
  if (collapsed) {
    return null;
  }

  const effectiveWidth = width > 0 ? width : defaultWidth;

  return (
    <div
      className={cn("shrink-0 overflow-hidden", className)}
      style={{ width: effectiveWidth }}
    >
      {children}
    </div>
  );
}
