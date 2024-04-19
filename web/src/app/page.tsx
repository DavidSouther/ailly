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
      <div
        style={{
          position: "fixed",
          left: 0,
          bottom: 0,
        }}
      >
        <pre>{`storyItem: ${store.state.storyItem}\nselections: ${JSON.stringify(
          store.state.selections
        )}`}</pre>
      </div>
      <aside style={{ flex: 1 }} />
      <section style={{ flex: 2 }}>
        <Storyboard {...store} />
      </section>
      <aside style={{ flex: 0.5 }} />
      <section style={{ flex: 3 }}>
        <Editor {...store}/>
      </section>
      <aside style={{ flex: 0.5 }} />
      <section style={{ flex: 2 }}>
        <Response {...store}/>
      </section>
      <aside style={{ flex: 1 }} />
    </div>
  );
}
