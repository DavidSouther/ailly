/**
 * Self-contained stand-in for galaxy's `../badges/badge` (Badge only).
 * Simplified: no AlertDialog confirm-remove, no download action, no
 * truncate-with-tooltip -- plain span-based badge with icon + text.
 */

import type { ReactNode } from "react";
import React from "react";

import { BadgeColor } from "../colors";
import type { IconType } from "../icons";
import { cn } from "../utils";

const COLOR_CLASS: Record<BadgeColor, string> = {
  [BadgeColor.SKY]: "bg-sky-100 text-sky-800",
  [BadgeColor.MINT]: "bg-emerald-100 text-emerald-800",
  [BadgeColor.AMBER]: "bg-amber-100 text-amber-800",
  [BadgeColor.BERRY]: "bg-rose-100 text-rose-800",
  [BadgeColor.PLUM]: "bg-purple-100 text-purple-800",
  [BadgeColor.METAL]: "bg-gray-100 text-gray-800",
  [BadgeColor.METAL_DARK]: "bg-gray-300 text-gray-900",
};

interface BadgeProps {
  children?: ReactNode;
  color?: BadgeColor | undefined;
  outlined?: boolean | undefined;
  textSize?: "sm" | "base" | undefined;
  icon?: IconType | undefined;
  iconColor?: string | undefined;
  className?: string | undefined;
}

export function Badge({
  children,
  color = BadgeColor.METAL,
  outlined = false,
  textSize = "base",
  icon,
  iconColor,
  className,
}: BadgeProps) {
  const Icon = icon;
  return (
    <div
      className={cn(
        "inline-flex w-fit flex-row items-center gap-0.5 truncate whitespace-nowrap rounded px-1 py-0",
        outlined ? "border bg-transparent" : COLOR_CLASS[color],
        textSize === "sm" && "text-xs",
        className,
      )}
    >
      {Icon ? <Icon className={cn("mr-0.5 h-3 w-3", iconColor)} /> : null}
      <span className="truncate">{children}</span>
    </div>
  );
}
