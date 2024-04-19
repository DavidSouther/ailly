"use client";
import { useAillyPageStore } from "./store";

import styles from "./storyboard.module.css";

export const Storyboard = () => {
  const { state, actions } = useAillyPageStore();
  return (
    <ol className={styles.blocks}>
      {
        /* state.storyItem > -1 && */
        state.story
          /* .slice(0, state.storyItem) */
          .map((item, block) => (
            <li key={item.slug} className={styles.block}>
              <h2>{item.title}</h2>
              <ol>
                {item.options.map((option, opt) => {
                  const name = item.slug + "-" + option.slug;
                  const input = item.select == "single" ? "radio" : "checkbox";
                  const checked = state.selections[block].includes(opt);
                  return (
                    <li key={option.slug}>
                      <label htmlFor={name} className={styles.option}>
                        <input
                          type={input}
                          id={name}
                          name={item.slug}
                          defaultChecked={checked}
                          onClick={() => actions.select(block, opt)}
                        />{" "}
                        {option.content}
                      </label>
                    </li>
                  );
                })}
              </ol>
            </li>
          ))
      }
    </ol>
  );
};
