import { describe, expect, it } from "vitest";
import { claude3 } from "./prompt-builder";

describe("bedrock claude3", () => {
  describe("prompt builder", () => {
    it("combines system prompts", () => {
      const actual = claude3([
        { role: "system", content: "sysa" },
        { role: "system", content: "sysb" },
      ]);
      expect(actual.system).toEqual("sysa\nsysb\n");
    });

    it("combines user prompts", () => {
      const actual = claude3([
        { role: "user", content: "usera" },
        { role: "user", content: "userb" },
      ]);
      expect(actual.messages).toEqual([
        { role: "user", content: "usera\nuserb" },
      ]);
    });

    it("combines assistant prompts", () => {
      const actual = claude3([
        { role: "assistant", content: "assista" },
        { role: "assistant", content: "assistb" },
      ]);
      expect(actual.messages).toEqual([
        { role: "assistant", content: "assista\nassistb" },
      ]);
    });
  });
});
