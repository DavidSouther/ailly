import { dirname } from "node:path";

import type { Content } from "../content/content.js";
import { isDefined } from "../util.js";
import type { Message } from "./index.js";

export async function addContentMessages(
  content: Content,
  context: Record<string, Content>,
) {
  content.meta ??= {};
  if (content.context.folder)
    content.meta.messages = getMessagesFolder(content, context);
  else content.meta.messages = getMessagesPredecessor(content, context);
  const messages = content.meta.messages;
  if (
    messages.at(-1)?.role === "assistant" &&
    (content.context.edit || !content.meta.continue)
  ) {
    messages.splice(-1, 1);
  }
  if (content.context.edit) {
    const lang = content.context.edit.file.split(".").at(-1) ?? "";
    messages.push({
      role: "assistant",
      content: `\`\`\`${lang}`,
      tokens: Number.NaN,
    });
  }
}

export function getMessagesPredecessor(
  content: Content,
  context: Record<string, Content>,
): Message[] {
  const system = (content.context.system ?? [])
    .map((s) => s.content)
    .join("\n");
  const history: Content[] = [];
  while (content) {
    history.push(content);
    // biome-ignore lint/style/noParameterAssign: Collate content up the stack
    content = context[content.context.predecessor ?? -1];
  }
  history.reverse();
  const augment: Message[] = history.flatMap<Message>((c) =>
    (c.context.augment ?? []).map<Message>(({ content }) => ({
      role: "user",
      content,
      tokens: Number.NaN,
    })),
  );
  const parts: Message[] = history
    .flatMap<Message | undefined>((content) => [
      {
        role: "user",
        content: content.prompt,
      },
      content.response
        ? { role: "assistant", content: content.response }
        : undefined,
    ])
    .filter(isDefined);

  return [
    { role: "system", content: system } satisfies Message,
    ...augment,
    ...parts,
  ];
}

export function getMessagesFolder(
  content: Content,
  context: Record<string, Content>,
): Message[] {
  // TODO: move this to a template
  const system = `${(content.context.system ?? [])
    .map((s) => s.content)
    .join(
      "\n",
    )}\nInstructions are happening in the context of this folder:\n<folder name="${
    content.meta?.root ?? dirname(content.path)
  }">\n${(content.context.folder ?? [])
    .map((c) => context[c])
    .map<string>(
      (c) =>
        `<file name="${c.name}">\n${
          c.meta?.text ?? `${c.prompt}\n${c.response}`
        }</file>`,
    )
    .join("\n")}\n</folder>`;

  const history: Content[] = [content];
  const augment: Message[] = [];

  const parts: Message[] = history
    .flatMap<Message | undefined>((content) => [
      {
        role: "user",
        content: content.prompt,
        tokens: Number.NaN,
      },
      content.response
        ? { role: "assistant", content: content.response, tokens: Number.NaN }
        : undefined,
    ])
    .filter(isDefined);
  return [
    { role: "system", content: system, tokens: Number.NaN },
    ...augment,
    ...parts,
  ];
}
