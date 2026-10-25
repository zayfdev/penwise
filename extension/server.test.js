/**
 * The capture flow against a fake Penwise server: the exact requests Chrome would make, with no Chrome.
 *
 * Run with `bun test extension`.
 */

import { describe, expect, test } from "bun:test";
import { publishCapture, sendAnchors } from "./server.js";

/** Starts a server that records every request and answers like Penwise. */
function fakeServer() {
  const calls = [];
  const server = Bun.serve({
    port: 0,
    async fetch(request) {
      const url = new URL(request.url);
      const body = request.method === "POST" ? await request.text() : "";
      calls.push({ method: request.method, path: url.pathname, query: url.search, body });
      if (url.pathname === "/assets") {
        return Response.json({ asset: "a".repeat(64), width: 1280, height: 800 });
      }
      if (url.pathname === "/pages") {
        return Response.json({ page: { id: "pg_0123456789abcdef" }, meta: {} });
      }
      if (url.pathname.endsWith("/images")) {
        return Response.json({ id: "img_1" });
      }
      if (url.pathname.endsWith("/anchors")) {
        return Response.json({ stored: 1 });
      }
      if (url.pathname.endsWith("/select")) {
        return Response.json({ active: "pg_0123456789abcdef" });
      }
      return new Response("unexpected", { status: 404 });
    },
  });
  return { calls, server, settings: { url: `http://127.0.0.1:${server.port}`, token: "t0ken" } };
}

describe("publishing a capture", () => {
  test("uploads, creates a page of the picture's size, places it, then selects it", async () => {
    const { calls, server, settings } = fakeServer();
    try {
      const result = await publishCapture(settings, {
        png: new Blob([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], { type: "image/png" }),
        width: 1280,
        height: 800,
        url: "https://example.com/pricing",
        title: "Pricing",
      });
      expect(result.page).toBe("pg_0123456789abcdef");
      expect(calls.map((call) => call.path)).toEqual([
        "/assets",
        "/pages",
        "/pages/pg_0123456789abcdef/images",
        "/pages/pg_0123456789abcdef/select",
      ]);
      const page = JSON.parse(calls[1].body);
      expect(page.kind).toEqual({ t: "fixed", width: 1280, height: 800 });
      expect(page.title).toBe("Pricing");
      const placed = JSON.parse(calls[2].body);
      expect(placed).toMatchObject({ asset: "a".repeat(64), w: 1280, h: 800, label: "https://example.com/pricing" });
      expect(calls[0].query).toContain("token=t0ken");
    } finally {
      server.stop(true);
    }
  });

  test("a picked element also stores its anchor", async () => {
    const { calls, server, settings } = fakeServer();
    try {
      await publishCapture(settings, {
        png: new Blob([new Uint8Array([0x89, 0x50, 0x4e, 0x47])]),
        width: 300,
        height: 120,
        url: "https://example.com/",
        title: "#cta",
        anchors: [{ selector: "#cta", rect: [10, 20, 300, 120], role: "button", text: "Get started" }],
      });
      const anchorCall = calls.find((call) => call.path.endsWith("/anchors"));
      expect(anchorCall).toBeDefined();
      expect(JSON.parse(anchorCall.body).anchors[0].selector).toBe("#cta");
      // Anchors land before the page is selected, so the agent sees both at once.
      expect(calls.at(-1).path.endsWith("/select")).toBe(true);
    } finally {
      server.stop(true);
    }
  });

  test("a refused upload surfaces the server's message", async () => {
    const server = Bun.serve({
      port: 0,
      fetch: () => new Response("not a PNG", { status: 400 }),
    });
    try {
      await expect(
        publishCapture({ url: `http://127.0.0.1:${server.port}`, token: "" }, {
          png: new Blob(["nope"]),
          width: 10,
          height: 10,
          url: "https://example.com/",
        }),
      ).rejects.toThrow(/not a PNG/);
    } finally {
      server.stop(true);
    }
  });
});

describe("anchors", () => {
  test("are sent as the server's shape", async () => {
    const { calls, server, settings } = fakeServer();
    try {
      await sendAnchors(settings, "pg_0123456789abcdef", [
        { selector: ".hero", rect: [0, 0, 100, 50], url: "https://example.com/" },
      ]);
      expect(calls[0].path).toBe("/pages/pg_0123456789abcdef/anchors");
      expect(JSON.parse(calls[0].body)).toEqual({
        anchors: [{ selector: ".hero", rect: [0, 0, 100, 50], url: "https://example.com/" }],
      });
    } finally {
      server.stop(true);
    }
  });
});
