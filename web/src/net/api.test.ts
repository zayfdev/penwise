import { describe, expect, it } from "vitest";
import { Api, ApiError, exchangePairingCode, pageBody } from "./api";
import { clearToken, resolveToken, syncUrl, TOKEN_KEY } from "./credentials";

interface Seen {
  url: string;
  init: RequestInit | undefined;
}

function fakeFetch(reply: (url: string) => Response, seen: Seen[] = []): typeof fetch {
  return ((input: RequestInfo | URL, init?: RequestInit) => {
    seen.push({ url: String(input), init });
    return Promise.resolve(reply(String(input)));
  }) as typeof fetch;
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

describe("pageBody", () => {
  it("maps templates onto the /pages body", () => {
    expect(pageBody("ruled", null, "")).toEqual({ background: "ruled" });
    expect(pageBody("infinite", "nb", "Idea")).toEqual({
      title: "Idea",
      notebook: "nb",
      kind: { t: "infinite" },
    });
  });
});

describe("Api", () => {
  it("sends the token header and JSON bodies; returns the created page id", async () => {
    const seen: Seen[] = [];
    const api = new Api(
      () => "tok",
      fakeFetch(() => json({ page: { id: "P1" } }), seen),
    );
    expect(await api.createPage("grid", "nb", "T")).toBe("P1");
    const call = seen[0];
    expect(call?.url).toBe("/pages");
    expect(call?.init?.method).toBe("POST");
    const headers = call?.init?.headers as Record<string, string> | undefined;
    expect(headers?.["X-Penwise-Token"]).toBe("tok");
    expect(JSON.parse(String(call?.init?.body))).toEqual({
      title: "T",
      notebook: "nb",
      background: "grid",
    });
  });

  it("reads text and search results defensively", async () => {
    const api = new Api(
      () => "t",
      fakeFetch((url) =>
        url.startsWith("/text")
          ? json({ text: "hello" })
          : json({ hits: [{ page: "p", title: "t", snippet: "s" }, 7] }),
      ),
    );
    expect(await api.pageText("p", false)).toBe("hello");
    expect(await api.search("x")).toEqual([
      { page: "p", title: "t", snippet: "s" },
      { page: "", title: "", snippet: "" },
    ]);
  });

  it("turns HTTP errors into ApiError with the status", async () => {
    const api = new Api(
      () => "t",
      fakeFetch(() => new Response("no", { status: 401 })),
    );
    await expect(api.pageText("p", false)).rejects.toMatchObject({ status: 401 });
    await expect(api.asset("h")).rejects.toBeInstanceOf(ApiError);
  });

  it("puts the token in the QR image URL (an <img> cannot send headers)", () => {
    const url = new Api(() => "a b").pairQrUrl("n1");
    expect(url).toBe("/pair.png?token=a+b&size=6&n=n1");
  });

  it("exchanges a pairing code for the token", async () => {
    const seen: Seen[] = [];
    const token = await exchangePairingCode(
      "123456",
      fakeFetch(() => json({ ok: true, token: "T" }), seen),
    );
    expect(token).toBe("T");
    expect(JSON.parse(String(seen[0]?.init?.body))).toEqual({ code: "123456" });
    await expect(
      exchangePairingCode(
        "x",
        fakeFetch(() => new Response("bad", { status: 401 })),
      ),
    ).rejects.toMatchObject({ status: 401 });
  });
});

describe("credentials", () => {
  const memory = () => {
    const map = new Map<string, string>();
    return {
      getItem: (k: string) => map.get(k) ?? null,
      setItem: (k: string, v: string) => void map.set(k, v),
      removeItem: (k: string) => void map.delete(k),
    };
  };

  it("takes the token from ?token= and remembers it", () => {
    const storage = memory();
    expect(resolveToken("", storage)).toBeNull();
    expect(resolveToken("?token=abc", storage)).toBe("abc");
    expect(storage.getItem(TOKEN_KEY)).toBe("abc");
    expect(resolveToken("", storage)).toBe("abc");
    clearToken(storage);
    expect(resolveToken("", storage)).toBeNull();
  });

  it("derives the socket URL from the page origin", () => {
    expect(syncUrl({ protocol: "http:", host: "h:7420" })).toBe("ws://h:7420/sync");
    expect(syncUrl({ protocol: "https:", host: "h" })).toBe("wss://h/sync");
  });
});

// The 32-hex id the core reports for a frame (`hex`).
const HEX = "019b09e4dcbb3b4544607a3701dfc6aa";

describe("Api frame routes", () => {
  const stillBody = {
    version: 3,
    doc: [390, 2000],
    viewport: [390, 844],
    strips: [{ y: 0, h: 2000, url: "/frames/F1/still/3/0.png" }],
    layout: { rects: {}, state: "" },
  };

  it("fetches a still with If-None-Match and reads the ETag", async () => {
    const seen: Seen[] = [];
    const api = new Api(
      () => "tok",
      fakeFetch(
        () => new Response(JSON.stringify(stillBody), { status: 200, headers: { ETag: '"s3"' } }),
        seen,
      ),
    );
    const reply = await api.still(HEX, '"s2"');
    expect(seen[0]?.url).toBe(`/frames/${HEX}/still`);
    expect(seen[0]?.init?.headers).toMatchObject({
      "X-Penwise-Token": "tok",
      "If-None-Match": '"s2"',
    });
    expect(reply.kind).toBe("fresh");
    if (reply.kind === "fresh") expect(reply.etag).toBe('"s3"');
  });

  it("maps 503 to a retry after the server's Retry-After", async () => {
    const busy = (retry: string | null) =>
      new Api(
        () => "t",
        fakeFetch(
          () =>
            new Response("capturing", {
              status: 503,
              headers: retry ? { "Retry-After": retry } : {},
            }),
        ),
      );
    expect(await busy("2").still(HEX, null)).toEqual({ kind: "retry", afterMs: 2_000 });
    expect(await busy(null).still(HEX, null)).toEqual({ kind: "retry", afterMs: 2_000 });
    expect(await busy("0").still(HEX, null)).toEqual({ kind: "retry", afterMs: 1_000 });
    expect(await busy("600").still(HEX, null)).toEqual({ kind: "retry", afterMs: 60_000 });
    expect(await busy("soon").still(HEX, null)).toEqual({ kind: "retry", afterMs: 2_000 });
  });

  it("maps 304 to unchanged and 404 or junk to no still", async () => {
    const unchanged = new Api(
      () => "t",
      fakeFetch(() => new Response(null, { status: 304 })),
    );
    expect(await unchanged.still(HEX, '"s"')).toEqual({ kind: "unchanged" });
    const missing = new Api(
      () => "t",
      fakeFetch(() => json({}, 404)),
    );
    expect(await missing.still(HEX, null)).toEqual({ kind: "none" });
    const junk = new Api(
      () => "t",
      fakeFetch(() => json({ nope: true })),
    );
    expect(await junk.still(HEX, null)).toEqual({ kind: "none" });
  });

  it("asks the server for a hit by document point and reads the rect", async () => {
    const seen: Seen[] = [];
    const api = new Api(
      () => "tok",
      fakeFetch(() => json({ key: "v1|a|b", selector: "a", rect: [10, 3000, 80, 20] }), seen),
    );
    expect(await api.frameHit(HEX, 100.4, 3010.6)).toEqual({
      key: "v1|a|b",
      rect: [10, 3000, 80, 20],
    });
    expect(seen[0]?.url).toBe(`/frames/${HEX}/hit?x=100&y=3011`);
    const failed = new Api(
      () => "t",
      fakeFetch(() => json({}, 500)),
    );
    expect(await failed.frameHit(HEX, 1, 1)).toEqual({ key: null, rect: null });
  });

  it("sends a round: true when sent, false on 409, an error otherwise", async () => {
    const seen: Seen[] = [];
    const ok = new Api(
      () => "tok",
      fakeFetch(() => json({ n: 2 }), seen),
    );
    expect(await ok.sendRound("prj 1")).toBe(true);
    expect(seen[0]?.url).toBe("/projects/prj%201/rounds/send");
    expect(seen[0]?.init?.method).toBe("POST");
    const none = new Api(
      () => "t",
      fakeFetch(() => json({}, 409)),
    );
    expect(await none.sendRound("p")).toBe(false);
    const broken = new Api(
      () => "t",
      fakeFetch(() => json({}, 500)),
    );
    await expect(broken.sendRound("p")).rejects.toBeInstanceOf(ApiError);
  });
});
