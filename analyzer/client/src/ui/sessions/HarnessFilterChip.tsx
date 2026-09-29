import { X } from "lucide-react";

import { Badge } from "../badges/badge";
import type { HarnessFilterChipProps } from "./searchQuery";

export function HarnessFilterChip({ label, onRemove }: HarnessFilterChipProps) {
  return (
    <span className="inline-flex items-center gap-0.5">
      <Badge textSize="sm">{label}</Badge>
      <button
        type="button"
        aria-label={`Remove ${label} filter`}
        onClick={onRemove}
        className="inline-flex items-center justify-center rounded p-0.5 text-foreground-muted hover:bg-background-hover-solid hover:text-foreground-title"
      >
        <X className="h-3 w-3" aria-hidden />
      </button>
    </span>
  );
}
