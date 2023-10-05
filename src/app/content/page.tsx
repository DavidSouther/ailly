"use client";

import { Card } from "@/components/Card";
import { Content, Summary } from "@/lib/content";

import ContentList from "./content_list";
import GenerateAll from "./generate_all";
import { useEffect, useState } from "react";

import { reloadContent } from "./server";

export default function ContentPage() {
  const [[content, summary], setState] = useState<
    readonly [Content[], Summary]
  >([[], { prompts: 0, tokens: 0 }]);

  const doReload = async () => {
    const content = await reloadContent();
    setState(content);
  };

  useEffect(() => {
    doReload();
  }, []);

  return (
    <div className="grid" style={{gridTemplateColumns: "2fr 1fr"}}>
      <Card header="Content list">
        <ContentList contents={content} />
      </Card>
      <div className="flex col">
        <Card header="Reload">
          <button onClick={doReload}>Reload</button>
        </Card>
        <Card header="Generation">
          <GenerateAll summary={summary} />
        </Card>
      </div>
    </div>
  );
}
