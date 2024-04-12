import { Content } from "../content/content.js";

export const DEFAULT_MODEL = "NOOP";
export const name = "noop";
export async function format(c: Content[]): Promise<void> {}
export async function generate<D extends {} = {}>(
  c: Content,
  _: unknown
): Promise<{ debug: D; message: string }> {
  return {
    message: `noop response for ${c.name}`,
    debug: { system: c.system } as unknown as D,
  };
}
export async function vector(s: string, _: unknown): Promise<number[]> {
  return [0.0, 1.0];
}
