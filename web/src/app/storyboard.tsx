"use client";
import { useMemo, useRef } from "react";
import { useAillyPageStore } from "./store";

import styles from "./storyboard.module.css";

const INPUT_DELAY = 900;
export const Storyboard = (store: ReturnType<typeof useAillyPageStore>) => {
  const { state, actions } = store;

  const timer = useRef<ReturnType<typeof globalThis.setTimeout>>();
  const onChange = useMemo(() => {
    return (block: number, opt: number) => {
      clearTimeout(timer.current);
      if (state.story[block].select === "multi") {
        timer.current = setTimeout(
          () => actions.select(block, opt),
          INPUT_DELAY
        );
      } else {
        actions.select(block, opt);
      }
    };
  }, [timer]);

  const editTimer = useRef<ReturnType<typeof globalThis.setTimeout>>();
  const onEdit = useMemo(() => {
    return (block: number, opt: number) => {
      clearTimeout(editTimer.current);
      editTimer.current = setTimeout(
        () => actions.select(block, opt),
        INPUT_DELAY
      );
    };
  }, [editTimer]);

  return (
    <section>
      <ol className={styles.blocks}>
        {state.storyItem > -1 &&
          state.story
            .slice(0, state.storyItem + 1)
            .map((item, block) => (
              <li key={item.slug} className={styles.block}>
                <h2>{item.title}</h2>
                <ol>
                  <p>{item.about}</p>
                  {item.options.map((option, opt) => {
                    const name = item.slug + "-" + option.slug;
                    const input =
                      item.select == "single" ? "radio" : "checkbox";
                    const checked =
                      state.selections[block]?.includes(opt) ?? false;
                    if (option.slug === "custom") {
                      if (state.storyItem >= state.story.length) {
                        return (
                          <li key={option.slug}>
                            <label htmlFor={name} className={styles.option}>
                              <input
                                type={input}
                                id={name}
                                name={item.slug}
                                defaultChecked={checked}
                                onClick={() => onChange(block, opt)}
                              />
                              <textarea
                                defaultValue={option.content}
                                placeholder={`Write your own ${item.title}`}
                                onChange={(e) => {
                                  const { value } = e.target;
                                  if (item.options.at(-1)?.slug !== "custom") {
                                    item.options.push({
                                      slug: "custom",
                                      content: value,
                                    });
                                  } else {
                                    item.options.at(-1)!.content = value;
                                  }
                                  onEdit(block, item.options.length - 1);
                                }}
                              ></textarea>
                            </label>
                          </li>
                        );
                      }
                    } else {
                      return (
                        <li key={option.slug}>
                          <label htmlFor={name} className={styles.option}>
                            <input
                              type={input}
                              id={name}
                              name={item.slug}
                              defaultChecked={checked}
                              onClick={() => onChange(block, opt)}
                            />{" "}
                            {option.content}
                          </label>
                        </li>
                      );
                    }
                  })}
                </ol>
              </li>
            ))
            .reverse()}
      </ol>
    </section>
  );
};
