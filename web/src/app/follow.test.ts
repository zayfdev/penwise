import { describe, expect, it } from "vitest";
import { MessageKind } from "../wasm";
import { choosePage, findPage, type NotebookGroup } from "./catalog";
import { planReceived, type ReceivedView, SnapshotTracker } from "./follow";

function received(partial: Partial<ReceivedView> & Pick<ReceivedView, "kind">): ReceivedView {
  return {
    page: undefined,
    changed_pages: [],
    live_pages: [],
    catalog_changed: false,
    scope: undefined,
    ...partial,
  };
}

function group(id: string, pageIds: string[]): NotebookGroup {
  return {
    notebook: { id, title: id, position: 0, page_count: pageIds.length } as never,
    pages: pageIds.map((p) => ({ id: p, notebook: id, title: p }) as never),
  };
}

describe("choosePage", () => {
  const groups = [group("nb1", []), group("nb2", ["a", "b"])];

  it("keeps the current page while it is live", () => {
    expect(choosePage(groups, "b", "a")).toBe("b");
  });
  it("falls back to the server's page, then the first page of any notebook", () => {
    expect(choosePage(groups, "gone", "b")).toBe("b");
    expect(choosePage(groups, null, "gone")).toBe("a");
    expect(choosePage([group("nb1", [])], null, null)).toBeNull();
  });
  it("findPage looks through every notebook", () => {
    expect(findPage(groups, "b")?.id).toBe("b");
    expect(findPage(groups, "zzz")).toBeNull();
    expect(findPage(groups, null)).toBeNull();
  });
});

describe("planReceived", () => {
  it("repaints committed only for the shown page and live only for live pages", () => {
    const plan = planReceived(
      received({ kind: MessageKind.OpsSince, changed_pages: ["p1"], live_pages: ["p2"] }),
      "p1",
      "follow",
      null,
      false,
    );
    expect([plan.repaintCommitted, plan.repaintLive]).toEqual([true, false]);
    const live = planReceived(
      received({ kind: MessageKind.StrokeDelta, live_pages: ["p1"] }),
      "p1",
      "follow",
      null,
      false,
    );
    expect([live.repaintCommitted, live.repaintLive]).toEqual([false, true]);
  });

  it("follows SwitchPage unless free; the first Welcome page is adopted only when nothing is shown", () => {
    const sw = received({ kind: MessageKind.SwitchPage, page: "p9" });
    expect(planReceived(sw, "p1", "follow", null, false).switchTo).toBe("p9");
    expect(planReceived(sw, "p1", "lead", null, false).switchTo).toBe("p9");
    expect(planReceived(sw, "p1", "free", null, false).switchTo).toBeNull();
    const welcome = received({ kind: MessageKind.Welcome, page: "p3" });
    expect(planReceived(welcome, null, "free", null, false).switchTo).toBe("p3");
    expect(planReceived(welcome, "p1", "follow", null, false).switchTo).toBeNull();
  });

  it("a finished catalog snapshot refreshes the lists and says the catalog is ready", () => {
    const end = received({ kind: MessageKind.SnapshotEnd });
    expect(planReceived(end, null, "follow", "catalog", false)).toMatchObject({
      refreshCatalog: true,
      catalogReady: true,
    });
    expect(planReceived(end, null, "follow", "p1", false)).toMatchObject({
      refreshCatalog: false,
      catalogReady: false,
    });
  });
});

describe("snapshots in progress", () => {
  it("do not repaint or refresh from half-applied state, but their end does", () => {
    const chunk = received({
      kind: MessageKind.SnapshotChunk,
      changed_pages: ["p1"],
      catalog_changed: true,
    });
    expect(planReceived(chunk, "p1", "follow", null, true)).toMatchObject({
      repaintCommitted: false,
      refreshCatalog: false,
    });
    const end = received({ kind: MessageKind.SnapshotEnd });
    expect(planReceived(end, "p1", "follow", "p1", false).repaintCommitted).toBe(true);
    expect(planReceived(end, "p1", "follow", "p2", false).repaintCommitted).toBe(false);
  });
});

describe("SnapshotTracker", () => {
  it("attributes SnapshotEnd to the scope its Begin announced", () => {
    const tracker = new SnapshotTracker();
    expect(tracker.inProgress).toBe(false);
    expect(tracker.observe({ kind: MessageKind.SnapshotBegin, scope: "catalog" })).toBeNull();
    expect(tracker.inProgress).toBe(true);
    expect(tracker.observe({ kind: MessageKind.SnapshotChunk, scope: undefined })).toBeNull();
    expect(tracker.observe({ kind: MessageKind.SnapshotEnd, scope: undefined })).toBe("catalog");
    expect(tracker.inProgress).toBe(false);
    expect(tracker.observe({ kind: MessageKind.SnapshotEnd, scope: undefined })).toBeNull();
  });
});
