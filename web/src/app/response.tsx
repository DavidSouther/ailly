"use client";

import M from "@/components/M";
import { useAillyPageStore } from "./store";
import styles from "./response.module.css";

export const Response = (store: ReturnType<typeof useAillyPageStore>) => {
  const { state } = store

  return (
    <>
      <div>
        <M>{state.response.content}</M>
      </div>
      <div>
        <button className={styles.button}>Generate</button>
      </div>
    </>
  );
};
