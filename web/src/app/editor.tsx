"use client";

import { useAillyPageStore } from "./store";

import styles from "./editor.module.css";
import { ChangeEvent, useCallback, useMemo, useRef } from "react";

const INPUT_DELAY = 900;
export const Editor = (store: ReturnType<typeof useAillyPageStore>) => {
  const { actions } = store;
  const input = useRef<HTMLTextAreaElement>(null);
  const timer = useRef<ReturnType<typeof globalThis.setTimeout>>();

  const send = useCallback(() => {
    clearTimeout(timer.current);
    if (input.current) actions.prompt(input.current?.value);
  }, [input]);

  const onChange = useMemo(() => {
    return (e: ChangeEvent<HTMLTextAreaElement>) => {
      clearTimeout(timer.current);
      timer.current = setTimeout(send, INPUT_DELAY);
    };
  }, []);

  return (
    <section className={styles.section}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          send();
          return false;
        }}
        action="#"
      >
        <textarea
          ref={input}
          className={styles.instruction}
          placeholder="What do we want to do today?"
          onChange={onChange}
          onBlur={send}
        />
        <button className={styles.button} onClick={send}>
          <span className="material-symbols-outlined">refresh</span>
        </button>
      </form>
    </section>
  );
};
