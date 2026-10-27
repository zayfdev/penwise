import { describe, expect, it } from "vitest";
import { Api, ApiError, exchangePairingCode, pageBody } from "./api";
import {
  clearToken,
  DEVICE_KEY,
  resolveDevice,
  resolveToken,
  syncUrl,
  TOKEN_KEY,
} from "./credentials";

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

  it("mints the device id once per tab session", () => {
    const storage = memory();
    let minted = 0;
    const wasm = {
      new_device_id: () => `dev${++minted}`,
    };
    const random = (b: Uint8Array) => b;
    expect(resolveDevice(storage, wasm, 1, random)).toBe("dev1");
    expect(resolveDevice(storage, wasm, 2, random)).toBe("dev1");
    expect(storage.getItem(DEVICE_KEY)).toBe("dev1");
  });

  it("derives the socket URL from the page origin", () => {
    expect(syncUrl({ protocol: "http:", host: "h:7420" })).toBe("ws://h:7420/sync");
    expect(syncUrl({ protocol: "https:", host: "h" })).toBe("wss://h/sync");
  });
});
