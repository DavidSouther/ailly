"use client";

import M from "@/components/M";
import { useAillyPageStore } from "./store";
import styles from "./response.module.css";
import { Tab, TabList } from "@/components/Tabs";

export const Response = (store: ReturnType<typeof useAillyPageStore>) => {
  const { state } = store;

  return (
    <section
      className={`${styles.response} ${state.generating ? "generating" : ""}`}
    >
      {state.generating ? (
        <div className={styles.spinner}></div>
      ) : (
        <TabList>
          <Tab title="Response">
            <M>{state.response.content}</M>
          </Tab>
          <Tab title="Prompt">
            <h2>System</h2>
            {state.content?.context.system?.map((s, i) => (
              <p key={i}>{s.content}</p>
            ))}
            <h2>User</h2>
            {state.content?.prompt}
          </Tab>
        </TabList>
      )}
    </section>
  );
};
