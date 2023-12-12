import { test, expect } from "vitest";
import { f32aNorm, similarity } from "./rag";

test("cosine similarity", () => {
  const a = new Float32Array([1, 2, 3]);
  const b = new Float32Array([3, 1, 2]);
  const an = f32aNorm(a);
  const bn = f32aNorm(b);

  expect(similarity(a, an, a, an)).toBe(1);
  expect(similarity(a, an, b, bn).toFixed(3)).toBe("0.786");
});
