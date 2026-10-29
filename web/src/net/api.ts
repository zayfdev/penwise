/**
 * HTTP side of the server: page/notebook creation, pairing, handwriting text, search, assets. Sync itself is
 * the `/sync` socket; these are the routes of `crates/penwise-server/src/api` the viewer calls. Responses are
 * JSON from the server and are checked before use (no trust in shapes).
 */

import { type Project, readProjects } from "../frames/model";

/** Page templates offered by "new page" and their `/pages` bodies. */
export type PageTemplate = "blank" | "ruled" | "grid" | "dots" | "infinite";

/** One `/search` hit. */
export interface SearchHit {
  readonly page: string;
  readonly title: string;
  readonly snippet: string;
}

/** The pairing code and QR source. */
export interface PairCode {
  readonly code: string;
  readonly expiresIn: number;
}

/** HTTP failure carrying the status (401 = bad token). */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

type Fetch = typeof fetch;

function record(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** Body of `POST /pages` for a template. */
export function pageBody(
  template: PageTemplate,
  notebook: string | null,
  title: string,
): Record<string, unknown> {
  const body: Record<string, unknown> = {};
  if (title !== "") body.title = title;
  if (notebook !== null && notebook !== "") body.notebook = notebook;
  if (template === "infinite") body.kind = { t: "infinite" };
  else body.background = template;
  return body;
}

/** Token-authenticated client. */
export class Api {
  constructor(
    private readonly token: () => string,
    private readonly fetchFn: Fetch = (input, init) => fetch(input, init),
  ) {}

  /** `POST /pages`; returns the new page id (it also arrives over `/sync`). */
  async createPage(
    template: PageTemplate,
    notebook: string | null,
    title: string,
  ): Promise<string> {
    const json = await this.json("POST", "/pages", pageBody(template, notebook, title));
    return text(record(record(json).page).id);
  }

  /** `POST /notebooks`. */
  async createNotebook(title: string): Promise<string> {
    const json = await this.json("POST", "/notebooks", { title });
    return text(record(record(json).notebook).id);
  }

  /** `GET /projects` — linked projects, their live ports and kind (frame URLs come from here). */
  async projects(): Promise<Project[]> {
    return readProjects(await this.json("GET", "/projects"));
  }

  /** `GET /text?page=…` — the page's handwriting as text (`refresh` re-reads it with the vision model). */
  async pageText(page: string, refresh: boolean): Promise<string> {
    const q = new URLSearchParams({ page });
    if (refresh) q.set("refresh", "true");
    return text(record(await this.json("GET", `/text?${q}`)).text);
  }

  /** `GET /search?q=…`. */
  async search(query: string): Promise<SearchHit[]> {
    const hits = record(
      await this.json("GET", `/search?${new URLSearchParams({ q: query })}`),
    ).hits;
    if (!Array.isArray(hits)) return [];
    return hits.map((h) => ({
      page: text(record(h).page),
      title: text(record(h).title),
      snippet: text(record(h).snippet),
    }));
  }

  /** `GET /pair/code`. */
  async pairCode(refresh: boolean): Promise<PairCode> {
    const json = record(await this.json(refresh ? "POST" : "GET", "/pair/code"));
    return { code: text(json.code), expiresIn: Number(json.expires_in) || 0 };
  }

  /** URL of the pairing QR image (the token rides in the query; `nonce` defeats caching). */
  pairQrUrl(nonce: string): string {
    return `/pair.png?${new URLSearchParams({ token: this.token(), size: "6", n: nonce })}`;
  }

  /** `GET /assets/{hash}`. */
  async asset(hash: string): Promise<Blob> {
    const response = await this.fetchFn(`/assets/${encodeURIComponent(hash)}`, {
      headers: { "X-Penwise-Token": this.token() },
    });
    if (!response.ok) throw new ApiError(response.status, `asset ${hash}: ${response.status}`);
    return response.blob();
  }

  private async json(method: string, path: string, body?: unknown): Promise<unknown> {
    const headers: Record<string, string> = { "X-Penwise-Token": this.token() };
    const init: RequestInit = { method, headers };
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
      init.body = JSON.stringify(body);
    }
    const response = await this.fetchFn(path, init);
    if (!response.ok) throw new ApiError(response.status, `${method} ${path}: ${response.status}`);
    return response.json();
  }
}

/** `POST /pair` — trades a pairing code for the access token (public route; the code is the credential). */
export async function exchangePairingCode(code: string, fetchFn: Fetch = fetch): Promise<string> {
  const response = await fetchFn("/pair", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code }),
  });
  if (!response.ok) throw new ApiError(response.status, "pairing code rejected");
  const token = text(record(await response.json()).token);
  if (token === "") throw new ApiError(500, "pairing answer has no token");
  return token;
}

/**
 * `GET /pair/local` — the token, when this viewer runs on the server's own machine (the desktop launcher
 * opens `http://127.0.0.1:7420/`). `null` anywhere else (the server answers 403) or when the server is away.
 */
export async function fetchLocalToken(fetchFn: Fetch = fetch): Promise<string | null> {
  try {
    const response = await fetchFn("/pair/local", { cache: "no-store" });
    if (!response.ok) return null;
    return text(record(await response.json()).token) || null;
  } catch {
    return null;
  }
}
