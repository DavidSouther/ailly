// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { HarnessFilterChip } from "../../src/ui/sessions/HarnessFilterChip";

afterEach(() => {
  cleanup();
});

describe("HarnessFilterChip", () => {
  it("renders the filter label", () => {
    render(<HarnessFilterChip label="harness: codex" onRemove={() => {}} />);

    expect(screen.getByText("harness: codex")).toBeInTheDocument();
  });

  it("exposes a remove button named for the filter", () => {
    render(<HarnessFilterChip label="harness: codex" onRemove={() => {}} />);

    expect(
      screen.getByRole("button", { name: /remove harness:\s*codex filter/i }),
    ).toBeInTheDocument();
  });

  it("calls onRemove when the remove button is clicked", async () => {
    const onRemove = vi.fn();
    render(<HarnessFilterChip label="harness: codex" onRemove={onRemove} />);

    await userEvent.click(screen.getByRole("button", { name: /remove harness:\s*codex filter/i }));

    expect(onRemove).toHaveBeenCalledOnce();
  });
});
