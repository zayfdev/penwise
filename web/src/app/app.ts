/**
 * Wiring of the viewer: wasm `Viewer`, the `/sync` connection, the page renderer, drawing and the panels.
 * No protocol or geometry logic lives here (that is wasm); this decides what to show and when to repaint.
 */

import { byId } from "../dom";
import { attachHistoryKeys } from "../input/keys";
import { PenInput } from "../input/pen";
import { Api, fetchLocalToken } from "../net/api";
import { Connection, type ConnectionState } from "../net/connection";
import { clearToken, resolveToken, TOKEN_KEY } from "../net/credentials";
import { browserOptions } from "../net/socket";
import { type DrawData, EMPTY_DRAW, readDrawList } from "../render/drawdata";
import { PaperGrain } from "../render/grain";
import { ImageCache } from "../render/images";
import { PageView } from "../render/pageview";
import { attachSidePanels } from "../ui/side-panels";
import { Sidebar } from "../ui/sidebar";
import { attachStage } from "../ui/stage";
import { StatusPill } from "../ui/status";
import { Toolbar } from "../ui/toolbar";
import { Unlock } from "../ui/unlock";
import { fitView, type Viewport } from "../view";
import { MessageKind, type Received, type Viewer, type Wasm } from "../wasm";
import { Actions } from "./actions";
import { choosePage, findPage, type NotebookGroup, readCatalog } from "./catalog";
import { type FollowMode, planReceived, SnapshotTracker } from "./follow";
import { historyState, stepHistory } from "./history";
import { exposeProbe } from "./probe";
import { Unsent } from "./unsent";

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
  private readonly unsent: Unsent;
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

  constructor(
    private readonly wasm: Wasm,
    device: string,
  ) {
    this.stage = byId("stage", HTMLElement);
    this.viewer = new wasm.Viewer();
    const entropy = new BigUint64Array(crypto.getRandomValues(new Uint8Array(8)).buffer)[0] ?? 1n;
    this.unsent = new Unsent(
      this.viewer,
      localStorage,
      device,
      byId("dead-panel", HTMLElement),
      (message) => this.status.problem(message),
      () => this.refreshStatus(),
    );
    this.unsent.restoreOrIdentify(device, entropy);
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
    const grain = new PaperGrain();
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
    attachSidePanels(
      this.api,
      () => this.current,
      (page) => this.userSelect(page),
    );
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
    window.addEventListener("beforeunload", (event) => this.unsent.warn(event));
    exposeProbe(this.pageView.stats, () => ({
      page: this.current,
      view: this.pageView.transform,
      connection: this.connState,
      pending: this.viewer.pending_ops(),
      head: this.viewer.head_seq().toString(),
    }));
  }

  /** Connects: a stored token, else the same-machine token, else asks for a pairing code. */
  async start(): Promise<void> {
    if (this.token === "") {
      const local = await fetchLocalToken();
      if (local !== null) {
        localStorage.setItem(TOKEN_KEY, local);
        this.token = local;
      }
    }
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
      const refused = this.token;
      clearToken(localStorage);
      this.token = "";
      // On the server's own machine a changed token is picked up again without a pairing code.
      void fetchLocalToken().then((local) => {
        if (local !== null && local !== refused) {
          localStorage.setItem(TOKEN_KEY, local);
          this.token = local;
          this.connection.start();
        } else {
          this.unlock.show("The server refused this token. Enter a new pairing code.");
        }
      });
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
    if (this.current !== null) {
      const flush = () => this.connection.flush();
      stepHistory(this.viewer, this.current, redo, flush, (m) => this.status.problem(m));
    }
    this.afterLocalEdit();
  }

  private refreshHistory(): void {
    this.toolbar.setHistory(...historyState(this.viewer, this.current));
  }

  private refreshStatus(): void {
    this.status.show(this.connState, this.viewer.pending_ops(), this.unsent.sync());
  }
}
