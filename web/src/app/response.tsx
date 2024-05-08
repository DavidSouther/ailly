"use client";

import M from "@/components/M";
import { useAillyPageStore } from "./store";
import styles from "./response.module.css";
import { Tab, TabList } from "@/components/Tabs";
import { Copy } from "@/components/Copy";

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
            {state.response.content && (
              <Copy contents={state.response.content}></Copy>
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
                "\t"
              )}
            ></Copy>
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
