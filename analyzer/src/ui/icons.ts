/**
 * Self-contained stand-in for galaxy's `../icons` (IconType only -- the
 * stories only use this as a type for lucide-react icon components).
 */

import type { ComponentType } from "react";

export type IconType = ComponentType<{ className?: string }>;
