"use client";

import M from "@/components/M";
import { useAillyPageStore } from "./store";
import styles from "./response.module.css";

export const Response = (store: ReturnType<typeof useAillyPageStore>) => {
  const { state } = store;

  return (
    <section
      className={`${styles.response} ${state.generating ? "generating" : ""}`}
    >
      {state.generating ? (
        <div className={styles.spinner}></div>
      ) : (
        <M>{state.response.content}</M>
      )}
    </section>
  );
};
