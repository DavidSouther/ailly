const mustache = require("mustache");
import { Content, View } from "./content.js";
import { META_PROMPT } from "./template_anthropic_metaprompt.js";
import { GRUG_PROMPT } from "./template_grug_prompt.js";
import { EDIT } from "./edit_template.js";
import { LOGGER } from "../index.js";
import * as YAML from "yaml";
import type { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs";

if (!global.structuredClone) {
  // TODO: Drop node 16 support
  global.structuredClone = (obj) => JSON.parse(JSON.stringify(obj));
}

export function mergeViews(...views: View[]): View {
  return views
    .map((v) => structuredClone(v))
    .reduce((a, b) => Object.assign(a, b), {});
}

// 10 is probably way too many to converge
const TEMPLATE_RECURSION_CONVERGENCE = 10;
export function mergeContentViews(
  c: Content,
  base: View,
  context: Record<string, Content>
) {
  if (c.context.view === false) return;
  c.meta = c.meta ?? {};
  c.meta.view = c.context.view;
  c.meta.prompt = c.prompt;
  if (c.context.predecessor && context[c.context.predecessor])
    mergeContentViews(context[c.context.predecessor], base, context);
  let view = structuredClone(base);
  for (const s of c.context.system ?? []) {
    if (s.view === false) continue;
    view = mergeViews(view, s.view);
    s.content = mustache.render(s.content, view);
  }
  view = mergeViews(view, c.context.view || {});
  let i = TEMPLATE_RECURSION_CONVERGENCE;
  while (i-- > 0) {
    let old = c.prompt;
    c.prompt = mustache.render(old, view);
    if (old == c.prompt) return;
  }
  LOGGER.warn(
    `Reached TEMPLATE_RECURSION_CONVERGENCE limit of ${TEMPLATE_RECURSION_CONVERGENCE}`
  );
}

export const GLOBAL_VIEW: View = {
  output: {
    explain: "Explain your thought process each step of the way.",
    verbatim:
      "Please respond verbatim, without commentary. Skip the preamble. Do not explain your reasoning.",
    prose: "Your output should be prose, with no additional formatting.",
    markdown: "Your output should use full markdown syntax.",
    python:
      "Your output should only contain Python code, within a markdown code fence:\n\n```py\n#<your code>\n```",
    edit: EDIT,
  },
  persona: {
    grug: GRUG_PROMPT,
    meta: META_PROMPT,
  },
};
/**
 * Read, parse, and validate a template view.
 */

export async function loadTemplateView(
  fs: FileSystem,
  ...paths: string[]
): Promise<View> {
  if (!paths) return {};
  let view = /* @type View */ {};
  for (const path of paths) {
    try {
      LOGGER.debug(`Reading template-view at ${path}`);
      const file = await fs.readFile(path);
      const parsed = YAML.parse(file);
      if (parsed && typeof parsed == "object") {
        view = { ...view, ...parsed };
      }
    } catch (err) {
      LOGGER.warn(`Failed to load template-view at ${path}`, { err });
    }
  }
  return view;
}
