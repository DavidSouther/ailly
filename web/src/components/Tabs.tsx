import {
  Children,
  PropsWithChildren,
  ReactNode,
  cloneElement,
  useId,
  useState,
} from "react";

import style from "./Tabs.module.css";

export const Tab = (
  props: PropsWithChildren<{
    title: ReactNode;
    parent?: string;
    checked?: boolean;
    onSelect?: () => void;
  }>
) => {
  const id = useId();
  const tab = `tab-${id}`;
  const panel = `panel-${id}`;
  return (
    <>
      <div role="tab" className={style.tab} id={tab} aria-controls={panel}>
        <label>
          {props.title}
          <input
            type="radio"
            name={props.parent}
            aria-controls={panel}
            value={tab}
            checked={props.checked}
            onChange={(e) => e.target.checked == true && props.onSelect!()}
          />
        </label>
      </div>
      <div
        role="tabpanel"
        className={style.tabpanel}
        id={panel}
        aria-labelledby={tab}
      >
        {props.children}
      </div>
    </>
  );
};

export const TabList = (props: { children: ReturnType<typeof Tab>[] }) => {
  const id = useId();
  const [selectedIndex, setSelectedIndex] = useState(0);
  return (
    <section
      role="tablist"
      className={style.tablist}
      style={{ "--tab-count": props.children.length }}
    >
      {Children.map(props.children, (child, index) =>
        cloneElement(child, {
          checked: index === selectedIndex,
          parent: id,
          idx: index,
          onSelect: () => setSelectedIndex(index),
        })
      )}
    </section>
  );
};
