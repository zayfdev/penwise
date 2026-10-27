/**
 * Wiring of the viewer: wasm `Viewer`, the `/sync` connection, the page renderer, drawing and the panels.
 * No protocol or geometry logic lives here (that is wasm); this decides what to show and when to repaint.
 */

import { byId } from "../dom";
import { attachHistoryKeys } from "../input/keys";
import { PenInput } from "../input/pen";
import { Api } from "../net/api";
import { Connection, type ConnectionState } from "../net/connection";
import { clearToken, resolveDevice, resolveToken } from "../net/credentials";
import { browserOptions } from "../net/socket";
import { type DrawData, EMPTY_DRAW, readDrawList } from "../render/drawdata";
import { GrainCache } from "../render/grain";
import { ImageCache } from "../render/images";
import { PageView } from "../render/pageview";
import { PairingPanel } from "../ui/pairing-panel";
import { SearchPanel } from "../ui/search-panel";
import { Sidebar } from "../ui/sidebar";
import { attachStage } from "../ui/stage";
import { StatusPill } from "../ui/status";
import { TextPanel } from "../ui/text-panel";
import { Toolbar } from "../ui/toolbar";
import { Unlock } from "../ui/unlock";
import { fitView, type Viewport } from "../view";
import { MessageKind, type Received, type Viewer, type Wasm } from "../wasm";
import { Actions } from "./actions";
import { choosePage, findPage, type NotebookGroup, readCatalog } from "./catalog";
import { type FollowMode, planReceived, SnapshotTracker } from "./follow";
import { exposeProbe } from "./probe";

/** All layers of a page (core `LayerSet` bits). */
const ALL_LAYERS = 0b111;

/** The running viewer. */
export class App {
  private readonly viewer: Viewer;
  private readonly api: Api;
  private readonly connection: Connection;
  private readonly pageView: PageView;
  private readonly pen: PenInput;
  private readonly sidebar: Sidebar;
  private readonly toolbar: Toolbar;
  private readonly status: StatusPill;
  private readonly unlock: Unlock;
  private readonly stage: HTMLElement;
  private readonly actions: Actions;
  private token = "";
  private groups: NotebookGroup[] = [];
  private current: string | null = null;
  private hint: string | null = null;
  private mode: FollowMode = "follow";
  private viewport: Viewport = { width: 1, height: 1 };
  private userMoved = false;
  private refitOnData = false;
  /** A page we just created: shown as soon as the catalog has it. */
  private pendingSelect: string | null = null;
  private connState: ConnectionState = "closed";
  private readonly snapshots = new SnapshotTracker();

  constructor(private readonly wasm: Wasm) {
    this.stage = byId("stage", HTMLElement);
    this.viewer = new wasm.Viewer();
    const random = (bytes: Uint8Array): Uint8Array => {
      crypto.getRandomValues(bytes as Uint8Array<ArrayBuffer>);
      return bytes;
    };
    const device = resolveDevice(sessionStorage, wasm, Date.now(), random);
    const entropy = new BigUint64Array(crypto.getRandomValues(new Uint8Array(8)).buffer)[0] ?? 1n;
    this.viewer.identify(device, entropy);
    this.token = resolveToken(location.search, localStorage) ?? "";
    this.api = new Api(() => this.token);
    this.actions = new Actions(this.api, {
      created: (id) => {
        this.pendingSelect = id;
        this.applyPendingSelect();
      },
      paired: (token) => {
        this.token = token;
        this.unlock.hide();
        this.connection.start();
      },
      problem: (message) => this.status.problem(message),
    });
    this.status = new StatusPill(byId("status", HTMLElement));
    this.unlock = new Unlock(byId("unlock", HTMLElement), (code) => this.actions.pair(code));
    const grain = new GrainCache(wasm);
    const images = new ImageCache(
      (hash) => this.api.asset(hash),
      () => this.pageView.invalidateCommitted(),
    );
    this.pageView = new PageView(
      byId("committed", HTMLCanvasElement),
      byId("live", HTMLCanvasElement),
      {
        page: () => findPage(this.groups, this.current),
        committed: () => this.drawData("committed"),
        live: () => this.drawData("live"),
      },
      grain,
      images,
    );
    this.pen = new PenInput({
      viewer: this.viewer,
      wasm,
      send: (frame) => this.connection.send(frame),
      flush: () => this.connection.flush(),
      page: () => this.current,
      view: () => this.pageView.transform,
      brush: () => this.toolbar.brush(),
      origin: () => this.stage.getBoundingClientRect(),
      liveChanged: () => this.pageView.invalidateLive(),
      committedChanged: () => this.afterLocalEdit(),
      now: () => Date.now(),
      problem: (message) => this.status.problem(message),
      schedule: (callback) => requestAnimationFrame(callback),
    });
    this.toolbar = new Toolbar(byId("toolbar", HTMLElement), wasm.preset_names(), {
      undo: () => this.step(false),
      redo: () => this.step(true),
      fit: () => this.refit(true),
      mode: (mode) => {
        this.mode = mode;
      },
    });
    this.sidebar = new Sidebar(byId("pages", HTMLElement), {
      select: (id) => this.userSelect(id),
      newPage: (template, notebook, title) =>
        void this.actions.createPage(template, notebook, title),
      newNotebook: (title) => void this.actions.createNotebook(title),
    });
    new TextPanel(byId("text-panel", HTMLElement), this.api, () => this.current);
    new SearchPanel(byId("search-panel", HTMLElement), this.api, (page) => this.userSelect(page));
    const pairing = new PairingPanel(byId("pair-panel", HTMLElement), this.api);
    byId("pair-toggle", HTMLButtonElement).addEventListener("click", () => {
      const panel = byId("pair-panel", HTMLElement);
      panel.hidden = !panel.hidden;
      if (!panel.hidden) void pairing.load(false);
    });
    this.connection = this.makeConnection();
    attachStage(this.stage, {
      pen: this.pen,
      view: () => this.pageView.transform,
      setView: (view) => {
        this.userMoved = true;
        this.pageView.setView(view);
      },
      resized: (viewport, dpr) => this.resized(viewport, dpr),
    });
    attachHistoryKeys(window, (redo) => this.step(redo));
    exposeProbe(this.pageView.stats, () => ({
      page: this.current,
      view: this.pageView.transform,
      connection: this.connState,
      pending: this.viewer.pending_ops(),
      head: this.viewer.head_seq().toString(),
    }));
  }

  /** Connects (or asks for a pairing code first). */
  start(): void {
    if (this.token === "")
      this.unlock.show("Enter the pairing code shown by the server or the tablet.");
    else this.connection.start();
  }

  private makeConnection(): Connection {
    return new Connection(
      this.viewer,
      this.wasm,
      {
        state: (state) => this.onState(state),
        received: (received) => this.onReceived(received),
        problem: (message) => this.status.problem(message),
      },
      browserOptions(() => this.token),
    );
  }

  private drawData(which: "committed" | "live"): DrawData {
    if (this.current === null) return EMPTY_DRAW;
    const list =
      which === "committed"
        ? this.viewer.render_page_paths(this.current, ALL_LAYERS)
        : this.viewer.render_live_paths(this.current);
    return readDrawList(list);
  }

  private onState(state: ConnectionState): void {
    this.connState = state;
    this.refreshStatus();
    if (state === "rejected") {
      clearToken(localStorage);
      this.token = "";
      this.unlock.show("The server refused this token. Enter a new pairing code.");
    }
  }

  private onReceived(received: Received): void {
    if (received.kind === MessageKind.Welcome && received.page) this.hint = received.page;
    const ended = this.snapshots.observe(received);
    const plan = planReceived(received, this.current, this.mode, ended, this.snapshots.inProgress);
    if (plan.refreshCatalog) this.groups = readCatalog(this.viewer);
    if (plan.catalogReady || (plan.refreshCatalog && !findPage(this.groups, this.current))) {
      const chosen = choosePage(this.groups, this.current, this.hint);
      if (chosen !== this.current) this.showPage(chosen);
    }
    if (
      plan.switchTo !== null &&
      plan.switchTo !== this.current &&
      findPage(this.groups, plan.switchTo)
    ) {
      this.showPage(plan.switchTo);
    }
    this.applyPendingSelect();
    if (plan.refreshCatalog) this.sidebar.render(this.groups, this.current);
    if (plan.repaintCommitted) {
      this.pageView.invalidateCommitted();
      if (this.refitOnData) this.refit(false);
    }
    if (plan.repaintLive) this.pageView.invalidateLive();
    this.refreshHistory();
    this.refreshStatus();
  }

  /** Opens the page we just created once the catalog has it (the broadcast may beat the HTTP answer). */
  private applyPendingSelect(): void {
    const page = this.pendingSelect;
    if (page === null || !findPage(this.groups, page)) return;
    this.pendingSelect = null;
    this.showPage(page);
  }

  private showPage(page: string | null): void {
    this.pen.abort();
    this.current = page;
    this.userMoved = false;
    this.refitOnData = true;
    this.connection.watch(page);
    this.sidebar.render(this.groups, page);
    this.refit(false);
    this.pageView.invalidateCommitted();
    this.pageView.invalidateLive();
    this.refreshHistory();
  }

  private userSelect(page: string): void {
    if (this.mode === "follow") {
      this.mode = "free";
      byId("tool-mode", HTMLSelectElement).value = "free";
    }
    this.showPage(page);
    if (this.mode === "lead") this.connection.send(this.wasm.encode_switch_page(page));
  }

  private refit(force: boolean): void {
    if (this.userMoved && !force) return;
    const page = findPage(this.groups, this.current);
    if (!page) return;
    const bounds = this.current ? this.viewer.content_bounds(this.current, ALL_LAYERS) : undefined;
    if (!page.fixed && bounds) this.refitOnData = false;
    if (page.fixed) this.refitOnData = false;
    this.userMoved = false;
    this.pageView.setView(fitView(page, bounds, this.viewport));
  }

  private resized(viewport: Viewport, dpr: number): void {
    this.viewport = viewport;
    this.pageView.resize(viewport, dpr);
    this.refit(false);
  }

  /** The pen committed a stroke (or undo/redo changed the page): repaint and refresh buttons. */
  private afterLocalEdit(): void {
    this.pageView.invalidateCommitted();
    this.refreshHistory();
    this.refreshStatus();
  }

  private step(redo: boolean): void {
    if (this.current === null) return;
    try {
      const count = redo
        ? this.viewer.redo(this.current, Date.now())
        : this.viewer.undo(this.current, Date.now());
      if (count > 0) this.connection.flush();
    } catch (error) {
      this.status.problem(`${redo ? "redo" : "undo"} failed: ${String(error)}`);
    }
    this.afterLocalEdit();
  }

  private refreshHistory(): void {
    const page = this.current;
    this.toolbar.setHistory(
      page !== null && this.viewer.can_undo(page),
      page !== null && this.viewer.can_redo(page),
    );
  }

  private refreshStatus(): void {
    this.status.show(this.connState, this.viewer.pending_ops());
  }
}
