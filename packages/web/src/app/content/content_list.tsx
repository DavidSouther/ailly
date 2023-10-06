import { Content } from "@ailly/core";
import { GenerateContent } from "./generate_all";

export default function ContentList({ contents }: { contents: Content[] }) {
  return (
    <>
      {contents.map((c) => (
        <details key={`${c.path}/${c.id}`}>
          <summary>{c.path.replace(process.cwd(), "")}</summary>
          <ContentDetail content={c} />
        </details>
      ))}
    </>
  );
}

function ContentDetail({ content }: { content: Content }) {
  const messages = content.messages ?? [];
  const [system, conversation] =
    messages.length > 0 && messages[0]?.role === "system"
      ? [messages[0], messages.slice(1)]
      : [undefined, messages];

  return (
    <div>
      <div>
        Tokens {content.tokens ?? "Unknown"}{" "}
        <GenerateContent content={content} />
      </div>
      {system && (
        <>
          <div>System</div>
          <div>
            <CodeBlock text={system.content} />
          </div>
        </>
      )}
      {conversation.map((m, i) => (
        <>
          <div key={`${i}-role`}>{m.role}</div>
          <div key={`${i}-content`}>
            <CodeBlock text={m.content} />
          </div>
        </>
      ))}
    </div>
  );
}

function CodeBlock({ text }: { text: string }) {
  const lines = text.split("\n").length;
  const rows = Math.min(Math.max(lines, 5), 30);
  return <textarea rows={rows} cols={80} disabled value={text}></textarea>;
}
