"use client";
import { Content, Summary } from "@ailly/core";
import { useState } from "react";
import { generateAllAction, generateOneAction, tuneAction } from "./server";

export default function GenerateAll({ summary }: { summary: Summary }) {
  const [status, setStatus] = useState<string>("Not yet run");

  async function generateAll() {
    let { message } = await generateAllAction();
    setStatus(message);
  }

  return (
    <>
      <ContentSummary summary={summary} />
      <button onClick={generateAll}>Generate All</button>
      {status}
    </>
  );
}

const GPT_35_COST = {
  tune: 0.008,
  out: 0.016,
  in: 0.012,
};

const GPT_4_COST = {
  out: 0.03,
  in: 0.06,
};

const COST = GPT_35_COST;
const OUT_LENGTH = 750;

export function ContentSummary({ summary }: { summary: Summary }) {
  const [outLength, setOutLength] = useState(OUT_LENGTH);
  const cost = Number(
    (outLength * COST.out + summary.tokens * COST.in) / 1000
  ).toFixed(6);
  return (
    <p>
      This generation set has {summary.tokens} total tokens in {summary.prompts}{" "}
      conversations. Using an output size of{" "}
      <input
        className="inline"
        style={{ width: "inherit" }}
        type="number"
        defaultValue={outLength}
        onChange={(e) => setOutLength(Number(e.target.value))}
      />
      tokens, this generation should cost ${cost}.
    </p>
  );
}

export function GenerateContent({ content }: { content: Content }) {
  function doGenerate() {
    generateOneAction(content);
  }
  return <button onClick={doGenerate}>Generate</button>;
}

export function TuneAll({ summary }: { summary: Summary }) {
  const [status, setStatus] = useState<string>("Not yet run");

  async function tuneAll() {
    let { message } = await tuneAction();
    setStatus(message);
  }

  return (
    <>
      <TuningSummary summary={summary} />
      <button onClick={tuneAll}>Fine Tune</button>
      {status}
    </>
  );
}

export function TuningSummary({ summary }: { summary: Summary }) {
  const cost = Number((summary.tokens * COST.tune) / 1000).toFixed(6);
  return (
    <p>
      This content set has {summary.tokens} total tokens in {summary.prompts}{" "}
      conversations. This fine tuning should cost ${cost}.
    </p>
  );
}
