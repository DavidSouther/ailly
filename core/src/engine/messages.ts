import { dirname } from "node:path";

import { Content } from "../content/content.js";
import { isDefined } from "../util.js";
import { Message } from "./index.js";

export async function addContentMessages(
  content: Content,
  context: Record<string, Content>
) {
  content.meta ??= {};
  if (content.context.folder)
    content.meta.messages = getMessagesFolder(content, context);
  else content.meta.messages = getMessagesPredecessor(content, context);
  let messages = content.meta.messages;
  if (
    messages.at(-1)?.role == "assistant" &&
    (content.context.edit || !content.meta.continue)
  ) {
    messages.splice(-1, 1);
  }
  if (content.context.edit) {
    const lang = content.context.edit.file.split(".").at(-1) ?? "";
    messages.push({ role: "assistant", content: "```" + lang, tokens: NaN });
  }
}

export function getMessagesPredecessor(
  content: Content,
  context: Record<string, Content>
): Message[] {
  const system = (content.context.system ?? [])
    .map((s) => s.content)
    .join("\n");
  const history: Content[] = [];
  while (content) {
    history.push(content);
    content = context[content.context.predecessor!];
  }
  history.reverse();
  const augment = history
    .map<Array<Message>>((c) =>
      (c.context.augment ?? []).map<Message>(({ content }) => ({
        role: "user",
        content,
        tokens: NaN,
      }))
    )
    .flat();
  const parts = history
    .map<Array<Message | undefined>>((content) => [
      {
        role: "user",
        content: content.prompt,
        tokens: NaN,
      },
      content.response
        ? { role: "assistant", content: content.response, tokens: NaN }
        : undefined,
    ])
    .flat()
    .filter(isDefined);
  return [
    { role: "system", content: system, tokens: NaN },
    ...augment,
    ...parts,
  ];
}

export function getMessagesFolder(
  content: Content,
  context: Record<string, Content>
): Message[] {
  // TODO: move this to a template
  const system =
    (content.context.system ?? []).map((s) => s.content).join("\n") +
    "\n" +
    "Instructions are happening in the context of this folder:\n" +
    `<folder name="${content.meta?.root ?? dirname(content.path)}">\n` +
    (content.context.folder ?? [])
      .map((c) => context[c])
      .map<string>(
        (c) =>
          `<file name="${c.name}">\n${
            c.meta?.text ?? c.prompt + "\n" + c.response
          }</file>`
      )
      .join("\n") +
    "\n</folder>";

  const history: Content[] = [content];
  const augment: Message[] = [];

  const parts = history
    .map<Array<Message | undefined>>((content) => [
      {
        role: "user",
        content: content.prompt,
        tokens: NaN,
      },
      content.response
        ? { role: "assistant", content: content.response, tokens: NaN }
        : undefined,
    ])
    .flat()
    .filter(isDefined);
  return [
    { role: "system", content: system, tokens: NaN },
    ...augment,
    ...parts,
  ];
}
