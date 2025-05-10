"use client";

import { Copy } from "@/components/Copy";
import M from "@/components/M";
import { Tab, TabList } from "@/components/Tabs";
import styles from "./response.module.css";
import type { useAillyPageStore } from "./store";

export const Response = (store: ReturnType<typeof useAillyPageStore>) => {
  const { state } = store;

  return (
    <section
      className={`${styles.response} ${state.generating ? "generating" : ""}`}
    >
      {state.generating ? (
        <div className={styles.spinner} />
      ) : (
        <TabList>
          <Tab title="Response">
            {state.response.content && (
              <Copy contents={state.response.content} />
            )}
            <M>{state.response.content}</M>
          </Tab>
          <Tab title="Prompt">
            <Copy
              contents={JSON.stringify(
                {
                  system: state.content?.context.system?.map((s) => s.content),
                  user: state.content?.prompt,
                },
                undefined,
                "\t",
              )}
            />
            <h2>System</h2>
            {state.content?.context.system?.map((s, i) => (
              <p key={s.content}>{s.content}</p>
            ))}
            <h2>User</h2>
            {state.content?.prompt}
          </Tab>
        </TabList>
      )}
    </section>
  );
};
