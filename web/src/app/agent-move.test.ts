import { describe, expect, it } from "vitest";
import { AGENT_DEFER_MS, chipText, moveLabel, OwnerDevices, shouldDefer } from "./agent-move";

describe("shouldDefer", () => {
  const now = 1_000_000;

  it("lets a move through when the owner has not drawn lately", () => {
    expect(shouldDefer("focus", false, null, now, false)).toBe(false);
    expect(shouldDefer("switch", false, now - AGENT_DEFER_MS, now, false)).toBe(false);
    expect(shouldDefer("focus", false, now - AGENT_DEFER_MS - 1, now, false)).toBe(false);
  });

  it("holds the agent's move for 10 s after the last stroke, and while one is open", () => {
    expect(shouldDefer("focus", false, now - 9_999, now, false)).toBe(true);
    expect(shouldDefer("switch", false, now - 1, now, false)).toBe(true);
    expect(shouldDefer("focus", false, null, now, true)).toBe(true);
  });

  it("never holds back the owner's own tablet, or a reconnect's Welcome", () => {
    expect(shouldDefer("focus", true, now - 1, now, true)).toBe(false);
    expect(shouldDefer("welcome", false, now - 1, now, true)).toBe(false);
  });
});

describe("OwnerDevices", () => {
  it("counts the devices seen inking as the owner's hands", () => {
    const devices = new OwnerDevices();
    expect(devices.has("tablet")).toBe(false);
    devices.inked("tablet");
    expect(devices.has("tablet")).toBe(true);
    expect(devices.has("agent")).toBe(false);
    expect(devices.has(null)).toBe(false);
  });
});

describe("moveLabel", () => {
  const frames = [
    { id: "f1", label: "bold title", route: "/work" },
    { id: "f2", label: "", route: "/about" },
  ];

  it("names the frame by its label, else its route", () => {
    expect(moveLabel("f1", frames, "Home")).toBe("bold title");
    expect(moveLabel("f2", frames, "Home")).toBe("/about");
  });

  it("falls back to the page's title, then to a plain word", () => {
    expect(moveLabel(null, frames, "Home")).toBe("Home");
    expect(moveLabel("unknown", frames, "Home")).toBe("Home");
    expect(moveLabel(null, [], null)).toBe("page");
    expect(moveLabel(null, [], "")).toBe("page");
  });

  it("makes the chip text", () => {
    expect(chipText("bold title")).toBe("Agent: open bold title →");
  });
});
