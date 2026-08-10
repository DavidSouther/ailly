/**
 * Self-contained stand-in for `@nominal-io/hooks`'s useResizeObserverEffect.
 * Simplified: no debounce support, no requestAnimationFrame batching --
 * just a plain ResizeObserver wired to a callback.
 */

import type { RefObject } from "react";
import { useEffect, useRef } from "react";

type ContentRect = ResizeObserverEntry["contentRect"];
type Callback = (contentRect?: ContentRect) => void;

interface Options {
  /** @default false */
  measureOnMount?: boolean;
}

export function useResizeObserverEffect<T extends HTMLElement = HTMLElement>(
  callback: Callback,
  ref: RefObject<T | null>,
  options: Options = {},
): void {
  const { measureOnMount = false } = options;
  const callbackRef = useRef(callback);
  callbackRef.current = callback;

  useEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    if (typeof window === "undefined" || !("ResizeObserver" in window)) {
      return undefined;
    }

    if (measureOnMount) {
      callbackRef.current(el.getBoundingClientRect());
    }

    const observer = new ResizeObserver((entries) => {
      callbackRef.current(entries[0]?.contentRect);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref, measureOnMount]);
}
