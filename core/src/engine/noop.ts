import { Content } from "../content/content.js";

export const DEFAULT_MODEL = "NOOP";
export const name = "noop";
export async function format(c: Content[]): Promise<void> {}
export async function generate<D extends {} = {}>(
  c: Content,
  _: unknown
): Promise<{ debug: D; message: string }> {
  await new Promise<void>((resolve) => {
    setTimeout(() => resolve(), 750);
  });
  return {
    message:
      process.env["AILLY_NOOP_RESPONSE"] ??
      `noop response for ${c.name}:\n${c.context.system
        ?.map((s) => s.content)
        .join("\n")}\n${c.prompt}`,
    debug: { system: c.context.system } as unknown as D,
  };
}
export async function vector(s: string, _: unknown): Promise<number[]> {
  return [0.0, 1.0];
}
