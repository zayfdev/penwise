/**
 * Frame iframes on the stage: paper canvas → frames → ink canvas.
 *
 * The iframes live in a wrapper that carries the canvas view as a CSS transform, so each iframe keeps its preset
 * size (`css_w × css_h`) and the site sees honest CSS pixels at any zoom. A chip per frame (preset, route, reload)
 * sits unscaled above the ink, so it stays readable.
 *
 * Input: the frames are click-through unless the pointer is a pen, "Draw with mouse" is on, or Ctrl/Meta is held
 * (for zooming). A cross-origin iframe cannot be inspected, so the last pointer type seen on the Penwise UI
 * decides — the wheel and mouse clicks then reach the page itself.
 */

import { el } from "../dom";
import type { ViewTransform } from "../view";
import { BridgeChannel } from "./bridge-link";
import {
  decodeBridge,
  type Hit,
  isFromFrame,
  type Layout,
  type PatchFailure,
  patchFor,
  patchMessage,
  type Rect,
} from "./bridge-messages";
import { type ChipBox, liftAbovePlaced } from "./chips";
import {
  diffFrames,
  type Frame,
  framesInteractive,
  frameTransform,
  frameUrl,
  intersects,
  PRESET_CHOICES,
  type Project,
  placeFrame,
} from "./model";
import { renderableFrames } from "./refresh";

/** Where the layer gets the client's own address. */
export interface FrameLayerEnv {
  /** Host the client used for sync (`location.hostname`). */
  host(): string;
  token(): string;
}

/** What the layer asks the app to do (the wasm edits). */
export interface FrameLayerEvents {
  preset(frame: Frame, preset: string): void;
  route(frame: Frame, route: string): void;
  /** The framed page reported its layout (contract K2 `pw:layout`); the app feeds it to the core. */
  layout(frame: Frame, layout: Layout): void;
  /** The owner picked a variant (`set_frame_chosen`). */
  choose(frame: Frame): void;
  /** The bridge applied/reverted a patch; `ok` false lists the DOM ops it could not apply. */
  patched(frame: Frame, ok: boolean, failed: readonly PatchFailure[]): void;
}

/** Height of a chip in CSS px: it sits just above its frame. */
const CHIP_HEIGHT = 30;

interface Placed {
  readonly root: HTMLDivElement;
  readonly iframe: HTMLIFrameElement;
  readonly chip: HTMLDivElement;
  readonly label: HTMLSpanElement;
  readonly preset: HTMLSelectElement;
  readonly route: HTMLInputElement;
  readonly ghostLabel: HTMLLabelElement;
  readonly ghostBox: HTMLInputElement;
  readonly useButton: HTMLButtonElement;
  frame: Frame;
  project: Project;
  src: string;
  /** Messages to and from the injected bridge (layout, watch, hit, replay, patch). */
  channel: BridgeChannel;
  /** Local "show the ghost patch" flag (default on, never synced). */
  ghost: boolean;
  /** Patch key last sent to the current document (`""` = none yet). */
  sentPatch: string;
}

/** The iframe/chip layer of the shown page. */
export class FrameLayer {
  /** Iframe builds/removals: a rebuild reloads the live site, so the e2e asserts it never happens. */
  readonly stats = { built: 0, removed: 0 };
  private readonly nodes = new Map<string, Placed>();
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
    window.addEventListener(
      "keydown",
      (event) => this.setModifier(event.ctrlKey || event.metaKey),
      true,
    );
    window.addEventListener(
      "keyup",
      (event) => this.setModifier(event.ctrlKey || event.metaKey),
      true,
    );
    window.addEventListener("blur", () => this.setModifier(false));
    window.addEventListener("message", (event) => this.onMessage(event));
  }

  /**
   * Asks the frame's bridge for the element under a frame-local viewport point (contract K2 `pw:hit?`). The
   * answer arrives while the stroke is still being drawn; `answer` gets no key if the frame is gone.
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
      if (project) this.update(frame, project);
      else this.move(frame);
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
        this.update(frame, project);
      }
    });
    this.markChosen(shown);
    this.applyInput();
    this.placeChips();
  }

  /** The owner's variant pick: the chosen frame is highlighted, its unchosen siblings dim to 60%. */
  private markChosen(frames: readonly Frame[]): void {
    const picked = new Set<string>();
    for (const frame of frames) if (frame.group !== null && frame.chosen) picked.add(frame.group);
    for (const frame of frames) {
      const node = this.nodes.get(frame.id);
      if (!node) continue;
      node.root.classList.toggle("chosen", frame.chosen);
      node.chip.classList.toggle("chosen", frame.chosen);
      node.root.classList.toggle(
        "dimmed",
        frame.group !== null && !frame.chosen && picked.has(frame.group),
      );
    }
  }

  private add(frame: Frame, project: Project): void {
    const root = el("div", { class: "frame" });
    const iframe = el("iframe", { attrs: { title: `${project.name} ${frame.route}` } });
    root.append(iframe);
    const label = el("span", { class: "name" });
    const preset = el(
      "select",
      { attrs: { "aria-label": "Frame preset" } },
      PRESET_CHOICES.map((choice) =>
        el("option", { text: choice.label, attrs: { value: choice.name } }),
      ),
    );
    const route = el("input", { attrs: { type: "text", "aria-label": "Frame route" } });
    const reload = el("button", { text: "Reload", attrs: { type: "button", class: "reload" } });
    const ghostBox = el("input", {
      attrs: { type: "checkbox", "aria-label": "Show the ghost patch" },
    });
    const ghostLabel = el("label", { class: "ghost" }, [ghostBox, "Ghost"]);
    const useButton = el("button", { text: "Use this", attrs: { type: "button", class: "use" } });
    const chip = el("div", { class: "frame-chip", attrs: { "data-frame": frame.id } }, [
      label,
      preset,
      route,
      reload,
      ghostLabel,
      useButton,
    ]);
    const node: Placed = {
      root,
      iframe,
      chip,
      label,
      preset,
      route,
      ghostLabel,
      ghostBox,
      useButton,
      frame,
      project,
      src: "",
      channel: new BridgeChannel(() => iframe.contentWindow),
      ghost: true,
      sentPatch: "",
    };
    preset.addEventListener("change", () => this.events.preset(node.frame, preset.value));
    route.addEventListener("change", () => this.events.route(node.frame, route.value));
    reload.addEventListener("click", () => this.reload(node));
    ghostBox.addEventListener("change", () => this.setGhost(node, ghostBox.checked));
    useButton.addEventListener("click", () => this.events.choose(node.frame));
    // A bridge starts with an empty watch list and no patch: both are sent again after every navigation
    // (the bridge's `pw:hello` does it too; a fresh document drops everything).
    iframe.addEventListener("load", () => {
      node.channel.loaded();
      node.sentPatch = "";
    });
    this.root.append(root);
    this.chips.append(chip);
    this.nodes.set(frame.id, node);
    this.stats.built += 1;
    this.update(frame, project);
  }

  /** Applies the item's box and controls (no URL work: the project may be temporarily unknown). */
  private move(frame: Frame): void {
    const node = this.nodes.get(frame.id);
    if (!node) return;
    node.frame = frame;
    node.root.style.left = `${Math.round(frame.x)}px`;
    node.root.style.top = `${Math.round(frame.y)}px`;
    node.root.style.width = `${frame.cssW}px`;
    node.root.style.height = `${frame.cssH}px`;
    if (node.preset.value !== frame.preset) node.preset.value = frame.preset;
    if (document.activeElement !== node.route) node.route.value = frame.route;
  }

  private update(frame: Frame, project: Project): void {
    const node = this.nodes.get(frame.id);
    const patchChanged = node !== undefined && node.frame.patch !== frame.patch;
    this.move(frame);
    if (!node) return;
    node.project = project;
    node.label.textContent =
      frame.label === "" ? `${project.name} ${frame.route}` : `${project.name} · ${frame.label}`;
    node.ghostLabel.hidden = frame.patch === null;
    node.ghostBox.checked = node.ghost;
    node.useButton.hidden = frame.group === null;
    node.useButton.disabled = frame.chosen;
    node.useButton.textContent = frame.chosen ? "Chosen" : "Use this";
    const src = frameUrl(this.env.host(), project.port, frame.route, this.env.token());
    if (src !== node.src) {
      node.src = src;
      node.iframe.src = src;
    }
    if (patchChanged) this.sendPatch(node);
  }

  /** The owner toggled the ghost: the patch is applied or reverted on the live page (local only). */
  private setGhost(node: Placed, on: boolean): void {
    node.ghost = on;
    this.sendPatch(node, true);
  }

  /**
   * Sends the frame's patch (applied while the ghost is on, reverted while it is off) unless the current
   * document already has that exact state. A frame whose patch was cleared reverts what is still applied.
   */
  private sendPatch(node: Placed, force = false): void {
    const send = patchFor(node.frame.patch, node.ghost);
    const key = send === null ? "none" : `${send.on ? "on" : "off"}:${send.patch}`;
    if (!force && key === node.sentPatch) return;
    const target = node.iframe.contentWindow;
    if (!target) return;
    if (send !== null) target.postMessage(patchMessage(send.patch, send.on), "*");
    else if (node.sentPatch !== "" && node.sentPatch !== "none") {
      target.postMessage(patchMessage("", false), "*");
    }
    node.sentPatch = key;
  }

  private remove(id: string): void {
    const node = this.nodes.get(id);
    if (!node) return;
    node.root.remove();
    node.chip.remove();
    this.nodes.delete(id);
    this.stats.removed += 1;
    // A stroke being drawn over it must still commit: it falls back to an area binding.
    node.channel.close();
  }

  /** The node whose window sent `message`; anything else (another frame, the app itself) is ignored. */
  private nodeFrom(source: unknown): Placed | null {
    for (const node of this.nodes.values()) {
      if (isFromFrame(source, node.iframe.contentWindow)) return node;
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
      node.channel.store(message.layout);
      this.events.layout(node.frame, message.layout);
    } else if (message.t === "hello") {
      // A (re)loaded document: the bridge lost its watch list and its patch.
      node.channel.loaded();
      this.sendPatch(node, true);
    } else if (message.t === "patched") {
      this.events.patched(node.frame, message.ok, message.failed);
    }
  }

  /** Chip position/size follow the frame; chips of frames outside the stage are hidden. */
  private placeChips(): void {
    const viewport = { width: this.root.clientWidth, height: this.root.clientHeight };
    const placed: ChipBox[] = [];
    for (const node of this.nodes.values()) {
      const box = placeFrame(node.frame, this.view);
      const visible = intersects(box, viewport);
      node.chip.hidden = !visible;
      if (!visible) continue;
      // Chips are unscaled: frames side by side would cover each other's controls, so lift the newcomer.
      const spot = liftAbovePlaced(
        {
          left: Math.round(Math.max(0, box.left)),
          top: Math.round(Math.max(2, box.top - CHIP_HEIGHT)),
          width: node.chip.offsetWidth,
        },
        CHIP_HEIGHT,
        placed,
      );
      node.chip.style.left = `${spot.left}px`;
      node.chip.style.top = `${spot.top}px`;
      placed.push(spot);
    }
  }

  /** A same-URL `src` assignment need not reload, so park on `about:blank` first. */
  private reload(node: Placed): void {
    node.iframe.src = "about:blank";
    node.iframe.src = node.src;
  }

  private setModifier(held: boolean): void {
    if (held === this.modifier) return;
    this.modifier = held;
    this.applyInput();
  }

  /** Frames take mouse/touch input unless a pen (or draw-with-mouse, or Ctrl for zooming) needs the stage. */
  private applyInput(): void {
    this.root.classList.toggle(
      "interact",
      framesInteractive(this.lastPointer, this.drawMouse, this.modifier),
    );
  }
}
