"use client";

import { useAillyPageStore } from "./store";

import styles from "./editor.module.css";

export const Editor = (store: ReturnType<typeof useAillyPageStore>) => {
  const { state, actions } = store;

  return (
    <div>
      <ol className={styles.prompt}>
        {state.selections
          .map((selections, block) =>
            selections.map((opt) => (
              <li key={`${block}-${opt}`}>
                <blockquote>
                  <p>{state.story[block].options[opt].content}</p>
                </blockquote>
              </li>
            ))
          )
          .flat()}
        <li>
          <textarea
            className={styles.instruction}
            placeholder="What do we want to do today?"
            onBlur={(e) => {
              actions.prompt(e.target.value);
            }}
          />
        </li>
      </ol>
    </div>
  );
};
