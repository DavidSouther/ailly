/**
 * Self-contained stand-in for galaxy's `../images` (Avatar, AvatarFallback,
 * AvatarRoot). Simplified: no radix-ui, no tooltip, plain span-based avatar.
 */

import { UserIcon } from "lucide-react";
import type { CSSProperties, HTMLAttributes, ReactNode } from "react";
import React from "react";

import { cn } from "./utils";

interface AvatarRootProps extends HTMLAttributes<HTMLSpanElement> {
  size?: number;
  border?: boolean;
  children?: ReactNode;
}

export function AvatarRoot({
  size = 32,
  border = true,
  className,
  style,
  children,
  ...rest
}: AvatarRootProps) {
  return (
    <span
      role="img"
      className={cn(
        "relative flex shrink-0 items-center justify-center overflow-hidden rounded-full align-middle select-none",
        border && "border",
        className,
      )}
      style={{
        height: size,
        width: size,
        minWidth: size,
        minHeight: size,
        ...(style as CSSProperties),
      }}
      {...rest}
    >
      {children}
    </span>
  );
}

interface AvatarFallbackProps extends HTMLAttributes<HTMLSpanElement> {
  children?: ReactNode;
  size?: number;
}

export function AvatarFallback({ children, size, className, style, ...rest }: AvatarFallbackProps) {
  return (
    <span
      className={cn(
        "flex h-full w-full items-center justify-center bg-background-active-hover leading-3 font-medium text-foreground uppercase",
        className,
      )}
      style={{ height: "100%", width: "100%", ...(style as CSSProperties) }}
      {...rest}
    >
      {children ?? (
        <UserIcon
          className="text-foreground-muted"
          style={{ width: size ? size * 0.5 : 12, height: size ? size * 0.5 : 12 }}
        />
      )}
    </span>
  );
}

export interface AvatarProps {
  src?: string;
  alt?: string;
  size?: number;
  className?: string;
  style?: CSSProperties;
  border?: boolean;
}

export function Avatar({ src, alt, size = 32, className, style, border = true }: AvatarProps) {
  return (
    <AvatarRoot size={size} border={border} className={className} style={style}>
      {src ? (
        <img src={src} alt={alt ?? ""} className="block h-full w-full rounded-none object-cover" />
      ) : (
        <AvatarFallback size={size} className="rounded-none" />
      )}
    </AvatarRoot>
  );
}
