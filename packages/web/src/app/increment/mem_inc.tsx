"use client";
import { useEffect, useState } from "react";
import { compactStore, getStore, incStore } from "./server_mem";

export default function MemInc() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    (async () => {
      setCount(await getStore());
      // Compact 1 in 10 page loads
      if (Math.random() > 0.1) {
        compactStore();
      }
    })();
  }, [setCount]);
  const doInc = async () => {
    setCount(await incStore());
  };

  return <button onClick={doInc}>{count}</button>;
}
