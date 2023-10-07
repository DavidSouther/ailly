"use client";
import { useEffect, useState } from "react";
import { getCount, inc } from "./server_a";

export default function Increment() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    (async () => setCount(await getCount()))();
  }, [setCount]);
  const doInc = async () => {
    setCount(await inc());
  };
  return <button onClick={doInc}>{count}</button>;
}
