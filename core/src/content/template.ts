import mustache from "mustache";
import { Content, View } from "./content.js";
import { META_PROMPT } from "./template_anthropic_metaprompt.js";
import { GRUG_PROMPT } from "./template_grug_prompt.js";

export function mergeViews(...views: View[]): View {
  return views
    .map((v) => structuredClone(v))
    .reduce((a, b) => Object.assign(a, b), {});
}

export function mergeContentViews(c: Content, base: View) {
  if (c.view === false) return;
  c.meta = c.meta ?? {};
  c.meta.view = c.view;
  c.meta.prompt = c.prompt;
  if (c.predecessor) mergeContentViews(c.predecessor, base);
  let view = structuredClone(base);
  for (const s of c.system ?? []) {
    if (s.view === false) continue;
    view = mergeViews(view, s.view);
    s.view = false;
    s.content = mustache.render(s.content, view);
  }
  view = mergeViews(view, c.view);
  c.prompt = mustache.render(c.prompt, view);
  c.view = false;
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
  },
  persona: {
    grug: GRUG_PROMPT,
    meta: META_PROMPT,
  },
};
