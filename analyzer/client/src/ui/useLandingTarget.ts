import { useEffect, useRef, useState } from "react";

import { usePanelTabsIfPresent } from "./PanelTabs";

/**
 * Where a cross-lens hand-off lands. One lens names an event to land on and
 * switches tabs; the row with that id focuses itself, so a keyboard user arrives
 * on it and the browser scrolls it into view, and marks itself as the current
 * location so it is visibly highlighted rather than only focused.
 *
 * The mark outlives the target: the target is cleared the moment it is consumed,
 * so an ordinary later tab click cannot replay the landing, while the row a user
 * was just handed to stays highlighted for as long as they are looking at it.
 */
export function useLandingTarget<T extends HTMLElement>(eventId: string) {
  const tabs = usePanelTabsIfPresent();
  const clearTarget = tabs?.clearTarget;
  const ref = useRef<T | null>(null);
  const consumed = useRef(false);
  const [landed, setLanded] = useState(false);
  const isTarget = tabs?.target === eventId;

  useEffect(() => {
    if (isTarget) {
      setLanded(true);
    }
  }, [isTarget]);

  // Separate from the effect above so the row has already rendered the
  // `tabIndex` that makes it focusable at all before the focus is attempted.
  // Once, so nothing steals focus back on a later unrelated render.
  useEffect(() => {
    if (!landed || consumed.current) {
      return;
    }
    consumed.current = true;
    ref.current?.focus();
    clearTarget?.();
  }, [landed, clearTarget]);

  return { ref, landed };
}
