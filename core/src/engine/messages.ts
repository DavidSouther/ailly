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
  if (messages.at(-1)?.role == "assistant") {
    messages = messages.slice(0, -1);
  }
  let fence: undefined | string = undefined;
  if (content.context.edit) {
    const lang = content.context.edit.file.split(".").at(-1) ?? "";
    fence = "```" + lang;
    messages.push({ role: "assistant", content: fence });
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
    .map<Array<Message | undefined>>(
      (c) =>
        (c.context.augment ?? []).map<Message>(({ content }) => ({
          role: "user",
          content:
            "Use this code block as background information for format and style, but not for functionality:\n```\n" +
            content +
            "\n```\n",
        })) ?? []
    )
    .flat()
    .filter(isDefined);
  const parts = history
    .map<Array<Message | undefined>>((content) => [
      {
        role: "user",
        content: content.prompt,
      },
      content.response
        ? { role: "assistant", content: content.response }
        : undefined,
    ])
    .flat()
    .filter(isDefined);
  return [{ role: "system", content: system }, ...augment, ...parts];
}

export function getMessagesFolder(
  content: Content,
  context: Record<string, Content>
): Message[] {
  const system =
    (content.context.system ?? []).map((s) => s.content).join("\n") +
    "\n" +
    "Instructions are happening in the context of this folder:\n" +
    `<folder name="${content.meta?.root ?? dirname(content.path)}">\n` +
    (content.context.folder ?? [])
      .map((c) => context[c])
      .map<string>(
        (c) =>
          `<file name="${c.name}>\n${
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
      },
      content.response
        ? { role: "assistant", content: content.response }
        : undefined,
    ])
    .flat()
    .filter(isDefined);
  return [{ role: "system", content: system }, ...augment, ...parts];
}
