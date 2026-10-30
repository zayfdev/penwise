/**
 * Frames on the stage: paper canvas → frames → ink canvas.
 *
 * Each frame is a box of `css_w × box_h` (the whole document for full-length frames) in a wrapper that carries
 * the canvas view as a CSS transform ([`FrameBox`](box.ts)). The box shows the server's still as image strips.
 * Only the focused frame has an iframe, at the preset size (so the site sees honest CSS pixels and `100vh` is
 * the preset's), placed at the page's reported scroll — the core's live window. The previous live iframe stays
 * hidden for a fast return; older ones are removed. A chip per frame sits unscaled above the ink.
 *
 * Input: the frames are click-through unless the pointer is a pen, "Draw with mouse" is on, or Ctrl/Meta is held
 * (for zooming). A cross-origin iframe cannot be inspected, so the last pointer type seen on the Penwise UI
 * decides — the wheel and mouse clicks then reach the page itself.
 */

import type { Viewport, ViewTransform } from "../view";
import { FrameBox, type IframeStats } from "./box";
import {
  decodeBridge,
  type Hit,
  isFromFrame,
  type Layout,
  type PatchFailure,
  type Rect,
} from "./bridge-messages";
import { type ChipBox, liftAbovePlaced } from "./chips";
import type { LivePlan } from "./focus";
import type { WindowRect } from "./hit";
import {
  diffFrames,
  type Frame,
  framesInteractive,
  frameTransform,
  frameUrl,
  intersects,
  type Project,
  placeFrame,
} from "./model";
import { renderableFrames } from "./refresh";
import { visibleDocRange } from "./scroll-link";
import { boxNearScreen, type Still, stripWidth } from "./still";

/** Where the layer gets the client's own address. */
export interface FrameLayerEnv {
  /** Host the client used for sync (`location.hostname`). */
  host(): string;
  token(): string;
  now(): number;
}

/** What the layer asks the app to do (the wasm edits). */
export interface FrameLayerEvents {
  preset(frame: Frame, preset: string): void;
  route(frame: Frame, route: string): void;
  /** The live page reported its layout (contract K2 `pw:layout`); the app feeds it to the core. */
  layout(frame: Frame, layout: Layout): void;
  /** The live page scrolled by itself (not because it was asked to) by `dy` document px. */
  scrolled(frame: Frame, dy: number): void;
  /** The owner picked a variant (`set_frame_chosen`). */
  choose(frame: Frame): void;
  /** The bridge applied/reverted a patch; `ok` false lists the DOM ops it could not apply. */
  patched(frame: Frame, ok: boolean, failed: readonly PatchFailure[]): void;
}

/** Height of a chip in CSS px: it sits just above its frame. */
const CHIP_HEIGHT = 30;

/** The box/chip layer of the shown page. */
export class FrameLayer {
  /** Iframe builds/removals: a rebuild reloads the live site, so the e2e asserts it never happens. */
  readonly stats: IframeStats = { built: 0, removed: 0 };
  private readonly nodes = new Map<string, FrameBox>();
  private view: ViewTransform = { scale: 1, originX: 0, originY: 0 };
  private lastPointer = "mouse";
  private drawMouse = false;
  private modifier = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly chips: HTMLElement,
    private readonly env: FrameLayerEnv,
    private readonly events: FrameLayerEvents,
  ) {
    this.applyInput();
    const modifier = (event: KeyboardEvent) => this.setModifier(event.ctrlKey || event.metaKey);
    window.addEventListener("keydown", modifier, true);
    window.addEventListener("keyup", modifier, true);
    window.addEventListener("blur", () => this.setModifier(false));
    window.addEventListener("message", (event) => this.onMessage(event));
  }

  /** True when mouse/touch input reaches frames (no pen, no "Draw with mouse", no zoom modifier). */
  get interactive(): boolean {
    return framesInteractive(this.lastPointer, this.drawMouse, this.modifier);
  }

  /** Size of the stage, CSS px. */
  get viewport(): Viewport {
    return { width: this.root.clientWidth, height: this.root.clientHeight };
  }

  /**
   * Asks the frame's bridge for the element under a viewport point of its live window (contract K2 `pw:hit?`).
   * The answer arrives while the stroke is still being drawn; `answer` gets no key if the frame is gone.
   */
  hit(frameId: string, x: number, y: number, answer: (hit: Hit) => void): void {
    const node = this.nodes.get(frameId);
    if (!node) {
      answer({ key: null, rect: null });
      return;
    }
    node.channel.hit(x, y, answer);
  }

  /** The last layout the bridge reported for the frame, or `null` before its first message. */
  layoutOf(frameId: string): Layout | null {
    return this.nodes.get(frameId)?.channel.layout() ?? null;
  }

  /**
   * Records rects the bridge sent outside a `pw:layout` (the hit answer) in the frame's layout, so the
   * merged rect survives the client's next layout send and is what the core binds against.
   */
  mergeRects(frameId: string, rects: Readonly<Record<string, Rect | null>>): Layout | null {
    return this.nodes.get(frameId)?.channel.mergeRects(rects) ?? null;
  }

  /** Tells the bridge which element keys the core needs rects for (`pw:watch`). */
  watch(frameId: string, keys: readonly string[]): void {
    this.nodes.get(frameId)?.channel.watch(keys);
  }

  /** Rebuilds a recorded page state in the frame (`pw:replay`). */
  replay(frameId: string, state: string): void {
    this.nodes.get(frameId)?.channel.replay(state);
  }

  /** The page state the frame is in now (`""` before the first layout or on the initial state). */
  stateOf(frameId: string): string {
    return this.nodes.get(frameId)?.channel.state() ?? "";
  }

  /** Frames whose bridge has reported a layout since their last load. */
  layoutCount(): number {
    let count = 0;
    for (const node of this.nodes.values()) if (node.channel.layout() !== null) count += 1;
    return count;
  }

  /** The frame whose iframe is shown, if any. */
  liveId(): string | null {
    for (const [id, node] of this.nodes) if (node.mode === "live") return id;
    return null;
  }

  /** Frames that hold an iframe (live and warm). */
  iframeCount(): number {
    let count = 0;
    for (const node of this.nodes.values()) if (node.iframe !== null) count += 1;
    return count;
  }

  /** The still images in a frame's box now (probe and e2e). */
  stillUrls(frameId: string): readonly string[] {
    return this.nodes.get(frameId)?.stills.urls ?? [];
  }

  /** Moves and scales the frames with the canvas view. */
  setView(view: ViewTransform): void {
    this.view = view;
    this.root.style.transform = frameTransform(view);
    this.placeChips();
  }

  /** The last pointer type seen on the window (`pen`, `mouse`, `touch`). */
  setPointer(type: string): void {
    if (type === this.lastPointer) return;
    this.lastPointer = type;
    this.applyInput();
  }

  /** "Draw with mouse": the mouse inks over frames instead of clicking through. */
  setDrawMouse(on: boolean): void {
    this.drawMouse = on;
    this.applyInput();
  }

  /**
   * Renders `frames` of the shown page (paint order) for the linked `projects`.
   *
   * A frame whose project is momentarily unknown (the first `GET /projects` has not answered, or a refetch
   * dropped it) keeps its live iframe: removing it would reload the framed site for nothing.
   */
  render(frames: readonly Frame[], projects: readonly Project[]): void {
    const known = new Map(projects.map((project) => [project.id, project] as const));
    const shown = renderableFrames(frames, projects, this.nodes.keys());
    const diff = diffFrames(
      [...this.nodes.values()].map((node) => node.frame),
      shown,
    );
    for (const id of diff.removed) this.remove(id);
    for (const frame of diff.updated) {
      const project = known.get(frame.project);
      if (project) this.describe(frame, project);
      else this.nodes.get(frame.id)?.move(frame);
    }
    for (const frame of diff.added) {
      const project = known.get(frame.project);
      if (project) this.add(frame, project);
    }
    shown.forEach((frame, index) => {
      const node = this.nodes.get(frame.id);
      if (!node) return;
      node.root.style.zIndex = String(index + 1);
      // A re-link can move the project to another port without touching the frame item.
      const project = known.get(frame.project);
      if (project && (node.project.port !== project.port || node.project.name !== project.name)) {
        this.describe(frame, project);
      }
    });
    this.markChosen(shown);
    this.applyInput();
    this.placeChips();
  }

  /** Gives the live frame its iframe, hides the previous one and removes the rest. */
  setLive(plan: LivePlan): void {
    for (const [id, node] of this.nodes) {
      node.setMode(id === plan.live ? "live" : id === plan.warm ? "warm" : "off");
    }
    this.placeChips();
  }

  /** Puts the frame's iframe at the live window `rect` (board px; `null` = the box's top-left). */
  placeWindow(frameId: string, rect: WindowRect | null): void {
    const node = this.nodes.get(frameId);
    if (!node) return;
    node.window = rect;
    node.placeIframe();
  }

  /**
   * Asks the live page to scroll to document y `y` (the camera moved). Not while the page just overrode a
   * request: then the page wins and only the window follows.
   */
  scrollTo(frameId: string, y: number): void {
    const node = this.nodes.get(frameId);
    if (node?.mode !== "live" || node.follower.held(this.env.now())) return;
    node.channel.scrollTo(0, y, node.follower.request(y));
  }

  /** The frame's current still (`null` clears it back to the placeholder). */
  setStill(frameId: string, still: Still | null): void {
    const node = this.nodes.get(frameId);
    if (node) node.still = still;
  }

  /**
   * Lays the strips that are on screen into each box, requested no wider than the screen shows them, and
   * leaves out those under a live window. A box farther than a stage-size from the stage releases its images.
   */
  renderStills(): void {
    const viewport = this.viewport;
    const margin = Math.max(viewport.width, viewport.height);
    const dpr = window.devicePixelRatio || 1;
    for (const node of this.nodes.values()) {
      const { frame } = node;
      node.root.classList.toggle("no-still", node.still === null);
      if (!boxNearScreen(placeFrame(frame, this.view), viewport, margin)) {
        node.stills.release();
        continue;
      }
      const full = frame.extent === "full";
      const visible = full ? visibleDocRange(frame.y, frame.boxH, this.view, viewport) : null;
      // Near but not on the stage: keep what is there (no churn while the owner pans back and forth).
      if (full && visible === null) continue;
      const win = node.mode === "live" ? node.window : null;
      node.stills.render(node.still, {
        cssW: frame.cssW,
        visible,
        covered:
          full && win !== null ? { from: win[1] - frame.y, to: win[1] - frame.y + win[3] } : null,
        width: stripWidth(frame.cssW, this.view.scale, dpr),
      });
    }
  }

  /** The owner's variant pick: the chosen frame is highlighted, its unchosen siblings dim to 60%. */
  private markChosen(frames: readonly Frame[]): void {
    const picked = new Set<string>();
    for (const frame of frames) if (frame.group !== null && frame.chosen) picked.add(frame.group);
    for (const frame of frames) {
      const node = this.nodes.get(frame.id);
      if (!node) continue;
      node.root.classList.toggle("chosen", frame.chosen);
      node.parts.chip.classList.toggle("chosen", frame.chosen);
      node.root.classList.toggle(
        "dimmed",
        frame.group !== null && !frame.chosen && picked.has(frame.group),
      );
    }
  }

  private add(frame: Frame, project: Project): void {
    const node = new FrameBox(frame, project, () => this.env.token(), this.stats, {
      preset: (f, preset) => this.events.preset(f, preset),
      route: (f, route) => this.events.route(f, route),
      choose: (f) => this.events.choose(f),
      interactive: () => this.interactive,
    });
    this.root.append(node.root);
    this.chips.append(node.parts.chip);
    this.nodes.set(frame.id, node);
    this.describe(frame, project);
  }

  private describe(frame: Frame, project: Project): void {
    const src = frameUrl(this.env.host(), project.port, frame.route, this.env.token());
    this.nodes.get(frame.id)?.describe(frame, project, src);
  }

  private remove(id: string): void {
    this.nodes.get(id)?.destroy();
    this.nodes.delete(id);
  }

  /** The node whose iframe sent `message`; anything else (another frame, the app itself) is ignored. */
  private nodeFrom(source: unknown): FrameBox | null {
    for (const node of this.nodes.values()) {
      if (isFromFrame(source, node.iframe?.contentWindow)) return node;
    }
    return null;
  }

  private onMessage(event: MessageEvent): void {
    const node = this.nodeFrom(event.source);
    if (!node) return;
    const message = decodeBridge(event.data);
    if (message === null) return;
    if (message.t === "hit") {
      node.channel.answer(message);
    } else if (message.t === "layout") {
      // A hidden (warm) page keeps running; its layout is not the frame's while it has no window.
      if (node.mode !== "live") return;
      node.channel.store(message.layout);
      const { scroll, cause, requestId } = message.layout;
      const dy = node.follower.reported(scroll[1], cause, requestId, this.env.now());
      this.events.layout(node.frame, message.layout);
      if (dy !== 0) this.events.scrolled(node.frame, dy);
    } else if (message.t === "hello") {
      // A (re)loaded document: the bridge lost its watch list and its patch.
      node.channel.loaded();
      node.sendPatch(true);
    } else if (message.t === "patched") {
      this.events.patched(node.frame, message.ok, message.failed);
    }
  }

  /** Chip position/size follow the frame; chips of frames outside the stage are hidden. */
  private placeChips(): void {
    const viewport = this.viewport;
    const placed: ChipBox[] = [];
    for (const node of this.nodes.values()) {
      const box = placeFrame(node.frame, this.view);
      const visible = intersects(box, viewport);
      const { chip } = node.parts;
      chip.hidden = !visible;
      if (!visible) continue;
      // Chips are unscaled: frames side by side would cover each other's controls, so lift the newcomer.
      const spot = liftAbovePlaced(
        {
          left: Math.round(Math.max(0, box.left)),
          top: Math.round(Math.max(2, box.top - CHIP_HEIGHT)),
          width: chip.offsetWidth,
        },
        CHIP_HEIGHT,
        placed,
      );
      chip.style.left = `${spot.left}px`;
      chip.style.top = `${spot.top}px`;
      placed.push(spot);
    }
  }

  private setModifier(held: boolean): void {
    if (held === this.modifier) return;
    this.modifier = held;
    this.applyInput();
  }

  /** Frames take mouse/touch input unless a pen (or draw-with-mouse, or Ctrl for zooming) needs the stage. */
  private applyInput(): void {
    this.root.classList.toggle("interact", this.interactive);
  }
}
