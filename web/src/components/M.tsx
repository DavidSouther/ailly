"use client";
import Markdown from "marked-react";

export default function M({ children }: { children?: string }) {
  return <Markdown>{children}</Markdown>;
}
