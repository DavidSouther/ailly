import { Clock, Layers } from "lucide-react";

import type { SessionListItem } from "../../tauri";
import { Badge } from "../badges/badge";
import { cn } from "../utils";
import {
  HARNESS_BADGE_COLOR,
  HARNESS_LABEL,
  eventCountLabel,
  lastActivityLabel,
  projectLabel,
} from "./format";

interface SessionListProps {
  sessions: SessionListItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export function SessionList({ sessions, selectedId, onSelect }: SessionListProps) {
  return (
    <ul className="flex flex-col divide-y border-y" aria-label="Sessions">
      {sessions.map((session) => (
        <li key={session.id}>
          <button
            type="button"
            aria-current={session.id === selectedId}
            onClick={() => onSelect(session.id)}
            className={cn(
              "flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-background-hover-solid",
              session.id === selectedId && "bg-background-active-solid",
            )}
          >
            <Badge color={HARNESS_BADGE_COLOR[session.harness]} textSize="sm">
              {HARNESS_LABEL[session.harness]}
            </Badge>
            <span className="min-w-0 flex-1 truncate font-medium text-foreground-title">
              {projectLabel(session.project)}
            </span>
            <span className="flex items-center gap-1 text-foreground-muted">
              <Layers size={12} />
              {eventCountLabel(session.event_count)}
            </span>
            <span className="flex items-center gap-1 whitespace-nowrap text-foreground-muted">
              <Clock size={12} />
              {lastActivityLabel(session.last_activity)}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}
