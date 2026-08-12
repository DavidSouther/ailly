import { useEffect, useState } from "react";

import { type AillyEvent, errorMessage, getEventPage } from "../tauri";

export enum LoadStatus {
  Idle = "idle",
  Loading = "loading",
  Error = "error",
  Ready = "ready",
}

export type LoadState =
  | { status: LoadStatus.Idle }
  | { status: LoadStatus.Loading }
  | { status: LoadStatus.Error; message: string }
  | { status: LoadStatus.Ready; events: AillyEvent[] };

/**
 * One bounded read of a session's events, shared by every lens on that session
 * so switching between them costs no extra round trip.
 */
export function useSessionEvents(sessionId: string | null): LoadState {
  const [state, setState] = useState<LoadState>({ status: LoadStatus.Idle });

  useEffect(() => {
    if (sessionId === null) {
      setState({ status: LoadStatus.Idle });
      return;
    }
    let cancelled = false;
    setState({ status: LoadStatus.Loading });
    getEventPage(sessionId)
      .then((events) => {
        if (!cancelled) {
          setState({ status: LoadStatus.Ready, events });
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setState({ status: LoadStatus.Error, message: errorMessage(cause) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  return state;
}
