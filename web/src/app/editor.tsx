"use client";

import { useAillyPageStore } from "./store";

import styles from "./editor.module.css";
import { ChangeEvent, useMemo } from "react";

const INPUT_DELAY = 900;
export const Editor = (store: ReturnType<typeof useAillyPageStore>) => {
  const { actions } = store;

  const onChange = useMemo(() => {
    let timer: ReturnType<typeof globalThis.setTimeout>;
    return (e: ChangeEvent<HTMLTextAreaElement>) => {
      clearTimeout(timer);
      timer = setTimeout(() => actions.prompt(e.target.value), INPUT_DELAY);
    };
  }, []);

  return (
    <section>
      <textarea
        className={styles.instruction}
        placeholder="What do we want to do today?"
        onChange={onChange}
      />
    </section>
  );
};
