/**
 * Self-contained stand-in for galaxy's `../typography` (BodyText, MutedText,
 * StrongText). Simplified: no forwardRef, plain span wrappers.
 */

import type { HTMLAttributes, ReactNode } from "react";
import React from "react";

import { cn } from "./utils";

interface Props extends HTMLAttributes<HTMLSpanElement> {
  children?: ReactNode;
  className?: string;
}

export function BodyText({ children, className, ...rest }: Props) {
  return (
    <span className={cn("m-0 text-foreground", className)} {...rest}>
      {children}
    </span>
  );
}

export function StrongText({ children, className, ...rest }: Props) {
  return (
    <span className={cn("m-0 font-medium text-foreground-title", className)} {...rest}>
      {children}
    </span>
  );
}

export function MutedText({ children, className, ...rest }: Props) {
  return (
    <span className={cn("m-0 font-medium text-foreground-muted", className)} {...rest}>
      {children}
    </span>
  );
}
