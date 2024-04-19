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
}

export interface StoryBook {
  slug: string;
  title: string;
  options: StoryBookOption[];
  select: "single" | "multi";
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
  const story: StoryBook[] = [ROLES, TONES];
  let storyItem = -1;
  let selections: number[][] = [];
  let instruction = "";
  let response = { content: "" };

  const reducers = {
    updateState(state: AillyPageState): AillyPageState {
      const newState = {
        story,
        instruction,
        selections: [...selections.map((opts) => [...opts])],
        storyItem,
        response: { ...response },
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
      if (storyItem == -1) storyItem = 0;
      this.updateAndGenerate();
    },
    select(block: number, choice: number) {
      if (selections[block + 1] == undefined) {
        storyItem = block + 1;
      }
      const newBlock =
        story[block].select == "multi"
          ? [...new Set([...selections[block], choice])]
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
            .map((opts, block) => opts.map((opt) => story[block]?.options[opt]))
            .flat()
            .map(({ content }) => ({ content, view: {} })),
          view: {},
        },
      };
      const genContent = await generateOne(content);
      response.content = genContent.response ?? "";
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

const ROLE_PROGRAMMER =
  "You are a software engineer developing a tool to teach non-technical people how prompt engineering affects LLM outputs.";
const ROLE_TECH_WRITER =
  "You are a technical writer documenting educational tools that teach how to use LLM and practice prompt engineering.";
const ROLE_FIC_WRITER =
  "You are a fiction writer using short sci-fi stories to explore the interactions of LLMs with society.";
const TONE_PRECISE = "Your writing is precise and technical.";
const TONE_PRO = "Your writing is professional.";
const TONE_WHIMSY = "Your writing is whimsical.";

const ROLES: StoryBook = {
  title: "Role",
  slug: "role",
  options: [
    { content: ROLE_PROGRAMMER, slug: "programmer" },
    { content: ROLE_TECH_WRITER, slug: "tech-writer" },
    { content: ROLE_FIC_WRITER, slug: "fic-writer" },
  ],
  select: "single",
};
const TONES: StoryBook = {
  title: "Tone",
  slug: "tone",
  options: [
    { content: TONE_PRECISE, slug: "precise" },
    { content: TONE_PRO, slug: "pro" },
    { content: TONE_WHIMSY, slug: "whimsy" },
  ],
  select: "single",
};
