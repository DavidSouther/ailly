/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import type { Meta } from "@storybook/react";
import { BotIcon } from "lucide-react";
import React from "react";

import { AvatarFallback, AvatarRoot } from "../ui/images";
import { BodyText, MutedText, StrongText } from "../ui/typography";
import { cn } from "../ui/utils";

/**
 * Era 1: Linear Chat (2023-era).
 *
 * A plain, static replica of the earliest agent interfaces: a single vertical
 * scroll of alternating user/assistant turns. There is no tool use, no side
 * panels, no artifacts, and no background work — everything that happens in
 * this era happens because the user typed it into the box.
 */

interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

const turns: ChatTurn[] = [
  {
    role: "user",
    content:
      "I have a CSV of temperature readings from a thermal-vac test, one row " +
      "per second. Some rows are missing values. What's a reasonable way to " +
      "fill the gaps before I plot it?",
  },
  {
    role: "assistant",
    content:
      "For a slow-moving signal like a thermal-vac temperature trace, linear " +
      "interpolation is usually enough:\n\n" +
      "- If gaps are a few seconds or less, interpolate between the " +
      "surrounding points.\n" +
      "- If a gap is longer than the time constant of the sensor, leave it " +
      "blank rather than inventing a trend.\n" +
      "- Keep a copy of the raw data with the gaps intact so you can tell " +
      "real readings apart from filled ones later.\n\n" +
      "Want me to write out the interpolation step in pseudocode?",
  },
  {
    role: "user",
    content: "Yes, pseudocode is fine.",
  },
  {
    role: "assistant",
    content:
      "for i in 1..n:\n" +
      "  if value[i] is missing:\n" +
      "    prev = last non-missing value before i\n" +
      "    next = next non-missing value after i\n" +
      "    if (next.time - prev.time) <= max_gap:\n" +
      "      value[i] = lerp(prev, next, at time[i])\n" +
      "    else:\n" +
      "      leave value[i] missing\n\n" +
      "That keeps the interpolation local and avoids smoothing over a real " +
      "sensor dropout.",
  },
  {
    role: "user",
    content: "That makes sense, thanks.",
  },
  {
    role: "assistant",
    content: "Happy to help — good luck with the test.",
  },
];

function ChatAvatar({ role }: { role: ChatTurn["role"] }) {
  return (
    <AvatarRoot size={28} className="bg-background-active">
      <AvatarFallback size={28}>
        {role === "user" ? "Y" : <BotIcon className="h-3.5 w-3.5" />}
      </AvatarFallback>
    </AvatarRoot>
  );
}

function ChatTurnRow({ turn }: { turn: ChatTurn }) {
  const isAssistant = turn.role === "assistant";

  return (
    <div
      className={cn(
        "flex gap-3 border-b border-border px-6 py-5",
        isAssistant && "bg-background-hover",
      )}
    >
      <ChatAvatar role={turn.role} />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <StrongText className="text-xs tracking-wide uppercase">
          {isAssistant ? "Assistant" : "You"}
        </StrongText>
        <BodyText className="whitespace-pre-wrap">{turn.content}</BodyText>
      </div>
    </div>
  );
}

export function Default() {
  return (
    <div className="mx-auto flex max-w-2xl flex-col bg-background">
      <div className="border-b border-border px-6 py-4">
        <MutedText className="text-xs tracking-wide uppercase">
          Era 1 — Linear Chat (2023)
        </MutedText>
      </div>
      {turns.map((turn, index) => (
        <ChatTurnRow key={`${turn.role}-${index}`} turn={turn} />
      ))}
    </div>
  );
}

const meta: Meta = {
  title: "Examples/Agent Harness Sketches/1. Linear Chat",
};

export default meta;
