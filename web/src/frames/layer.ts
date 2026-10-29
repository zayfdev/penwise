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
  frame: Frame;
  project: Project;
  src: string;
}

/** The iframe/chip layer of the shown page. */
export class FrameLayer {
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

  /** Renders `frames` of the shown page (paint order) for the linked `projects`. */
  render(frames: readonly Frame[], projects: readonly Project[]): void {
    const known = new Map(projects.map((project) => [project.id, project] as const));
    const shown = frames.filter((frame) => known.has(frame.project));
    const diff = diffFrames(
      [...this.nodes.values()].map((node) => node.frame),
      shown,
    );
    for (const id of diff.removed) this.remove(id);
    for (const frame of diff.updated) this.update(frame, known.get(frame.project) as Project);
    for (const frame of diff.added) this.add(frame, known.get(frame.project) as Project);
    shown.forEach((frame, index) => {
      const node = this.nodes.get(frame.id);
      if (!node) return;
      node.root.style.zIndex = String(index + 1);
      // A re-link can move the project to another port without touching the frame item.
      const project = known.get(frame.project) as Project;
      if (node.project.port !== project.port || node.project.name !== project.name) {
        this.update(frame, project);
      }
    });
    this.applyInput();
    this.placeChips();
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
    const chip = el("div", { class: "frame-chip", attrs: { "data-frame": frame.id } }, [
      label,
      preset,
      route,
      reload,
    ]);
    const node: Placed = { root, iframe, chip, label, preset, route, frame, project, src: "" };
    preset.addEventListener("change", () => this.events.preset(node.frame, preset.value));
    route.addEventListener("change", () => this.events.route(node.frame, route.value));
    reload.addEventListener("click", () => this.reload(node));
    this.root.append(root);
    this.chips.append(chip);
    this.nodes.set(frame.id, node);
    this.update(frame, project);
  }

  private update(frame: Frame, project: Project): void {
    const node = this.nodes.get(frame.id);
    if (!node) return;
    node.frame = frame;
    node.project = project;
    node.root.style.left = `${Math.round(frame.x)}px`;
    node.root.style.top = `${Math.round(frame.y)}px`;
    node.root.style.width = `${frame.cssW}px`;
    node.root.style.height = `${frame.cssH}px`;
    if (node.preset.value !== frame.preset) node.preset.value = frame.preset;
    if (document.activeElement !== node.route) node.route.value = frame.route;
    node.label.textContent = `${project.name} ${frame.route}`;
    const src = frameUrl(this.env.host(), project.port, frame.route, this.env.token());
    if (src !== node.src) {
      node.src = src;
      node.iframe.src = src;
    }
  }

  private remove(id: string): void {
    const node = this.nodes.get(id);
    if (!node) return;
    node.root.remove();
    node.chip.remove();
    this.nodes.delete(id);
  }

  /** Chip position/size follow the frame; chips of frames outside the stage are hidden. */
  private placeChips(): void {
    const viewport = { width: this.root.clientWidth, height: this.root.clientHeight };
    for (const node of this.nodes.values()) {
      const box = placeFrame(node.frame, this.view);
      node.chip.style.left = `${Math.round(Math.max(0, box.left))}px`;
      node.chip.style.top = `${Math.round(Math.max(2, box.top - CHIP_HEIGHT))}px`;
      node.chip.hidden = !intersects(box, viewport);
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
