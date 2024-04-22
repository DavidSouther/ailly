"use client";
import { Editor } from "./editor";
import { Response } from "./response";
import { Storyboard } from "./storyboard";

import styles from "./page.module.css";
import { useAillyPageStore } from "./store";

export default function Home() {
  const store = useAillyPageStore();
  return (
    <div className={styles.main}>
      <Response {...store} />
      <Editor {...store} />
      <Storyboard {...store} />
    </div>
  );
}
