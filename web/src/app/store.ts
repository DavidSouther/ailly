"use client";
import { Dispatch, MutableRefObject, useMemo, useReducer, useRef } from "react";
import type { Content } from "@ailly/core/src/content/content";
import { generateOne } from "./ailly";

export interface AillyPageState {
  story: StoryBook[];
  storyItem: number;
  selections: number[][];
  instruction: string;
  response: Response;
  generating: boolean;
}

export interface StoryBook {
  slug: string;
  title: string;
  about: string;
  options: StoryBookOption[];
  select: "single" | "multi";
  format: (content: string) => string;
}

export interface StoryBookOption {
  slug: string;
  content: string;
}

export interface Prompt {
  content: string;
}

export interface Response {
  content: string;
}

export type AillyStoreDispatch = Dispatch<{
  action: keyof ReturnType<typeof makeAillyStore>["reducers"];
  payload?: unknown;
}>;

export function makeAillyStore(dispatch: MutableRefObject<AillyStoreDispatch>) {
  const story: StoryBook[] = [ROLES, TONES, BACKGROUND, OUTPUT];
  let storyItem = -1;
  let nextStoryItem = -1;
  let selections: number[][] = [];
  let instruction = "";
  let response = { content: "" };
  let generating = true;

  const reducers = {
    updateState(_: AillyPageState): AillyPageState {
      const newState = {
        story,
        instruction,
        selections: [...selections.map((opts) => [...opts])],
        storyItem,
        response: { ...response },
        generating,
      };
      return newState;
    },
  };

  const actions = {
    update() {
      dispatch.current({ action: "updateState" });
    },
    updateAndGenerate() {
      this.update();
      this.generate();
    },
    prompt(content: string) {
      instruction = content;
      if (storyItem == -1) nextStoryItem = 0;
      this.updateAndGenerate();
    },
    select(block: number, choice: number) {
      if (selections[block + 1] == undefined) {
        nextStoryItem = block + 1;
      }
      const newBlock =
        story[block].select == "multi"
          ? [...new Set(selections[block] ?? []), choice]
          : [choice];
      // Un-read-only selections
      selections = [...selections];
      selections[block] = newBlock;

      this.updateAndGenerate();
    },
    async generate() {
      const content: Content = {
        name: "dev",
        outPath: "/ailly/dev",
        path: "/ailly/dev",
        prompt: instruction,
        context: {
          system: selections
            .map((opts, block) =>
              opts.map((opt) =>
                (story[block]?.format ?? ((c) => c))(
                  story[block]?.options[opt].content
                )
              )
            )
            .flat()
            .map((content) => ({ content, view: {} })),
          view: {},
        },
      };
      generating = true;
      this.update();
      try {
        const genContent = await generateOne(content);
        const error = (genContent.meta?.debug as { error: {} } | undefined)
          ?.error;
        if (error) {
          response.content = JSON.stringify(error);
        } else {
          response.content = genContent.response ?? "";
        }
      } finally {
        generating = false;
      }
      this.update();
      await pause(1200);
      storyItem = nextStoryItem;
      this.update();
    },
  };

  const initialState: AillyPageState = (() => {
    return {
      story,
      storyItem,
      selections,
      instruction,
      response,
    } as AillyPageState;
  })();

  return { initialState, reducers, actions };
}

export function useAillyPageStore() {
  const dispatch = useRef<AillyStoreDispatch>(() => undefined);

  const { initialState, reducers, actions } = useMemo(
    () => makeAillyStore(dispatch),
    [dispatch]
  );

  const [state, dispatcher] = useReducer(reducers.updateState, initialState);
  dispatch.current = dispatcher;

  return { state, dispatch, actions };
}

const EXPLAIN_ROLE =
  "Setting a role in a language model prompt establishes context and boundaries that guide the model's response to align with the specified persona, tone, and intent.";
const ROLE_PROGRAMMER =
  "You are a software engineer developing a tool to teach non-technical people how prompt engineering affects LLM outputs.";
const ROLE_TECH_WRITER =
  "You are a technical writer documenting educational tools that teach how to use LLM and practice prompt engineering.";
const ROLE_FIC_WRITER =
  "You are a fiction writer using short sci-fi stories to explore the interactions of LLMs with society.";

const EXPLAIN_TONE =
  "The tone set in an LLM system prompt can significantly influence the style, formality, and overall linguistic characteristics of the model's generated output";
const TONE_PRECISE = "Your writing is precise and technical.";
const TONE_PRO = "Your writing is professional.";
const TONE_WHIMSY = "Your writing is whimsical.";

const EXPLAIN_BACKGROUND =
  "Providing relevant background information to a large language model (LLM) is crucial for contextualizing the task, enabling the model to leverage its knowledge base effectively, and generating more accurate and relevant responses tailored to the specific use case.";
const BACKGROUND_TEMPERATURE =
  "temperature: Affects the randomness of the response, with higher values leading to more varied outputs.";
const BACKGROUND_PROMPT_SINGLE =
  '`prompt`: Example with a single user message: `[{"role": "user", "content": "Hello, Claude"}]`';
const BACKGROUND_PROMPT_MULTI =
  '`prompt`: Example with multiple conversational turns: [ {"role": "user", "content": "Hello there."}, {"role": "assistant", "content": "Hi, I\'m Claude. How can I help you?"}, {"role": "user", "content": "Can you explain LLMs in plain English?"}]';
const BACKGROUND_MAX_TOKENS =
  "`max_tokens`: Controls the maximum length of the response.";

const EXPLAIN_OUTPUT =
  "LLMs are 'few-shot' learners, which means that given just one or a few examples they are able to repeat and reformulate responses accounting for that information. Describing the desired output will have the LLM often output in a matching format.";
const OUTPUT_HTML =
  "Output using HTML tags. Keep it simple - only use <p></p> tags for blocks, and <ul></ul> unordered lists.";
const OUTPUT_DOCBOOK =
  "Output using DocBook XML. Blocks of text go in <para></para> tags. Lists are in an <itemizedlist></itemizedlist> and list items go in <listitem></listitem>.";
const OUTPUT_JSON =
  "Output in JSON. Use an object structure with key `section` to show sections as an array, then objects with `block` and strings for a paragraph and `list` (and an array) for lists.";
const OUTPUT_MARKDOWN =
  "Output in Markdown. Do not use headings. Paragraphs are separated by a blank line. Prefix lists with *.";

const ROLES: StoryBook = {
  title: "Role",
  slug: "role",
  about: EXPLAIN_ROLE,
  options: [
    { content: ROLE_PROGRAMMER, slug: "programmer" },
    { content: ROLE_TECH_WRITER, slug: "tech-writer" },
    { content: ROLE_FIC_WRITER, slug: "fic-writer" },
  ],
  select: "single",
  format: (c) => `<role>${c}</role>`,
};
const TONES: StoryBook = {
  title: "Tone",
  slug: "tone",
  about: EXPLAIN_TONE,
  options: [
    { content: TONE_PRECISE, slug: "precise" },
    { content: TONE_PRO, slug: "pro" },
    { content: TONE_WHIMSY, slug: "whimsy" },
  ],
  select: "single",
  format: (c) => `<tone>${c}</tone>`,
};
const BACKGROUND: StoryBook = {
  title: "Background",
  slug: "background",
  about: EXPLAIN_BACKGROUND,
  options: [
    { content: BACKGROUND_MAX_TOKENS, slug: "max_tokens" },
    { content: BACKGROUND_TEMPERATURE, slug: "temperature" },
    { content: BACKGROUND_PROMPT_SINGLE, slug: "single" },
    { content: BACKGROUND_PROMPT_MULTI, slug: "many" },
  ],
  select: "multi",
  format: (c) => `<background>${c}</background>`,
};

const OUTPUT: StoryBook = {
  title: "Output",
  slug: "output",
  about: EXPLAIN_OUTPUT,
  options: [
    { content: OUTPUT_HTML, slug: "html" },
    { content: OUTPUT_DOCBOOK, slug: "xml" },
    { content: OUTPUT_JSON, slug: "json" },
    { content: OUTPUT_MARKDOWN, slug: "md" },
  ],
  select: "single",
  format: (c) => `<output>${c}</output>`,
};

async function pause(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
