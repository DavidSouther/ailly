// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";

/**
 * User story: Priya opens Ailly Analyzer on a machine set to dark mode. The
 * app should render with `<html data-theme="dark">` immediately, matching
 * the OS, without her needing to toggle anything. If she switches her OS to
 * light mode while the app is still open, the app should follow along live.
 */

type ChangeListener = (event: MediaQueryListEvent) => void;

class FakeMediaQueryList {
  matches: boolean;
  media = "(prefers-color-scheme: dark)";
  private listeners: ChangeListener[] = [];

  constructor(matches: boolean) {
    this.matches = matches;
  }

  addEventListener(type: string, listener: ChangeListener) {
    if (type === "change") this.listeners.push(listener);
  }

  removeEventListener(type: string, listener: ChangeListener) {
    if (type !== "change") return;
    this.listeners = this.listeners.filter((l) => l !== listener);
  }

  /** Simulates the OS flipping its appearance setting while the app is running. */
  emit(matches: boolean) {
    this.matches = matches;
    for (const listener of this.listeners) {
      listener({ matches } as MediaQueryListEvent);
    }
  }
}

let fakeMediaQueryList: FakeMediaQueryList;

function stubMatchMedia(initiallyDark: boolean) {
  fakeMediaQueryList = new FakeMediaQueryList(initiallyDark);
  window.matchMedia = ((query: string) => {
    if (query !== "(prefers-color-scheme: dark)") {
      throw new Error(`Unexpected media query in test: ${query}`);
    }
    return fakeMediaQueryList as unknown as MediaQueryList;
  }) as typeof window.matchMedia;
}

beforeEach(() => {
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
});

describe("Journey: Respect system light/dark settings", () => {
  it("sets data-theme to dark when the OS prefers dark", async () => {
    stubMatchMedia(true);

    const { syncThemeWithSystem } = await import("../../src/theme/systemTheme");
    syncThemeWithSystem();

    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("sets data-theme to light when the OS prefers light", async () => {
    stubMatchMedia(false);

    const { syncThemeWithSystem } = await import("../../src/theme/systemTheme");
    syncThemeWithSystem();

    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("updates data-theme live when the OS preference changes while running", async () => {
    stubMatchMedia(false);

    const { syncThemeWithSystem } = await import("../../src/theme/systemTheme");
    syncThemeWithSystem();
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");

    fakeMediaQueryList.emit(true);
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

    fakeMediaQueryList.emit(false);
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });
});
