import { describe, expect, it } from "vitest";
import { makePipelineSettings } from "..";
import { makeCLIContent } from "../content/content";
import { TIMEOUT, generate } from "./noop.js";

describe("noop engine", () => {
  it("closes the stream when finished", async () => {
    TIMEOUT.resetTimeout(0);
    const content = makeCLIContent({
      prompt: "Tell me a joke!",
      argContext: "none",
      context: {},
    });
    const settings = await makePipelineSettings({ root: "/root" });
    const generator = generate(content, settings);

    let next: string;
    const decoder = new TextDecoder("utf-8");
    const reader = generator.stream.getReader();
    const getNext = async () => {
      const read = await reader.read();
      return decoder.decode(read.value);
    };
    next = await getNext();
    expect(next).toBe("noop respo");
    next = await getNext();
    expect(next).toBe("nse for st");
    next = await getNext();
    expect(next).toBe("dout:\n\n\n[r");
    next = await getNext();
    expect(next).toBe("esponse] T");
    next = await getNext();
    expect(next).toBe("ell me a j");
    next = await getNext();
    expect(next).toBe("oke!");
    await generator.done;
    await reader.closed;
    expect(generator.message()).toBe(
      "noop response for stdout:\n\n\n[response] Tell me a joke!",
    );
  });
});
