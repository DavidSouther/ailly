import { useCallback } from "react";
import styles from "./Copy.module.css";

export const Copy = (props: { contents: string }) => {
  const copy = useCallback(() => {
    window.navigator.clipboard.writeText(props.contents);
  }, [props.contents]);

  return (
    <button className={styles.button} onClick={copy} type="button">
      <span className="material-symbols-outlined">content_copy</span>
    </button>
  );
};
