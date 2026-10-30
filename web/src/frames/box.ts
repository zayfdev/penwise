/**
 * One frame on the stage: its box (the still strips and, for the live and the previous frame, an iframe), its
 * chip, and the page-side state that belongs to the iframe (bridge channel, scroll follower, patch sent).
 *
 * The box is `css_w × box_h` in page units. The iframe keeps the preset size and sits at the core's live window.
 */

import { el } from "../dom";
import { BridgeChannel } from "./bridge-link";
import { patchFor, patchMessage } from "./bridge-messages";
import { buildChip, type ChipParts } from "./chip-ui";
import type { WindowRect } from "./hit";
import type { Frame, Project } from "./model";
import { iframeOffset, ScrollFollower } from "./scroll-link";
import type { Still } from "./still";
import { StillView } from "./still-view";

/** Who holds the frame's iframe: the live one, the hidden previous one, or nobody. */
export type LiveMode = "live" | "warm" | "off";

/** What a box reports to its owner. */
export interface BoxEvents {
  preset(frame: Frame, preset: string): void;
  route(frame: Frame, route: string): void;
  choose(frame: Frame): void;
  /** True when a mouse press on a still is a click on the frame rather than the start of a stroke. */
  interactive(): boolean;
}

/** Iframe builds and removals, shared by all boxes (a rebuild reloads the live site; the e2e watches it). */
export interface IframeStats {
  built: number;
  removed: number;
}

/** A frame's box and chip. */
export class FrameBox {
  readonly root = el("div", { class: "frame" });
  readonly stills: StillView;
  readonly parts: ChipParts;
  /** Messages to and from the injected bridge (layout, watch, hit, replay, patch). */
  readonly channel: BridgeChannel;
  /** The page's own scrolling versus scrolling this client asked for. */
  readonly follower = new ScrollFollower();
  /** Only the live and the previous frame have one. */
  iframe: HTMLIFrameElement | null = null;
  mode: LiveMode = "off";
  src = "";
  /** The live window in board px (the core's `frame_live_window`), `null` before a layout. */
  window: WindowRect | null = null;
  still: Still | null = null;
  /** Local "show the ghost patch" flag (default on, never synced). */
  ghost = true;
  /** Patch key last sent to the current document (`""` = none yet). */
  sentPatch = "";

  constructor(
    public frame: Frame,
    public project: Project,
    token: () => string,
    private readonly stats: IframeStats,
    events: BoxEvents,
  ) {
    this.root.dataset.frame = frame.id;
    this.root.dataset.live = "off";
    this.stills = new StillView(token);
    this.root.append(this.stills.root);
    this.parts = buildChip(frame.id);
    this.channel = new BridgeChannel(() => this.iframe?.contentWindow ?? null);
    const { preset, route, reload, ghostBox, useButton } = this.parts;
    preset.addEventListener("change", () => events.preset(this.frame, preset.value));
    route.addEventListener("change", () => events.route(this.frame, route.value));
    reload.addEventListener("click", () => this.reload());
    ghostBox.addEventListener("change", () => this.setGhost(ghostBox.checked));
    useButton.addEventListener("click", () => events.choose(this.frame));
    this.stills.root.addEventListener("pointerdown", (event) => {
      if (event.pointerType === "mouse" && events.interactive()) event.stopPropagation();
    });
  }

  /** Applies the item's box and controls (no URL work: the project may be temporarily unknown). */
  move(frame: Frame): void {
    this.frame = frame;
    const { style } = this.root;
    style.left = `${Math.round(frame.x)}px`;
    style.top = `${Math.round(frame.y)}px`;
    style.width = `${frame.cssW}px`;
    style.height = `${frame.boxH}px`;
    const { preset, route } = this.parts;
    if (preset.value !== frame.preset) preset.value = frame.preset;
    if (document.activeElement !== route) route.value = frame.route;
    this.placeIframe();
  }

  /** Chip texts and the iframe address for `frame` of `project` served at `src`. */
  describe(frame: Frame, project: Project, src: string): void {
    const patchChanged = this.frame.patch !== frame.patch;
    this.move(frame);
    this.project = project;
    const { label, ghostLabel, ghostBox, useButton } = this.parts;
    label.textContent =
      frame.label === "" ? `${project.name} ${frame.route}` : `${project.name} · ${frame.label}`;
    ghostLabel.hidden = frame.patch === null;
    ghostBox.checked = this.ghost;
    useButton.hidden = frame.group === null;
    useButton.disabled = frame.chosen;
    useButton.textContent = frame.chosen ? "Chosen" : "Use this";
    if (src !== this.src) {
      this.src = src;
      if (this.iframe !== null) this.iframe.src = src;
    }
    if (patchChanged) this.sendPatch();
  }

  /** Puts the iframe at the live window; it keeps the preset size. */
  placeIframe(): void {
    const { iframe, frame, window } = this;
    if (iframe === null) return;
    iframe.style.width = `${frame.cssW}px`;
    iframe.style.height = `${frame.cssH}px`;
    const { left, top } = iframeOffset(frame.x, frame.y, window);
    iframe.style.left = `${left}px`;
    iframe.style.top = `${top}px`;
  }

  /**
   * Live: shown; warm: kept for a fast return, invisible but laid out at the preset size (a `display: none`
   * page loses its layout, so scroll and size would be wrong on return); off: removed.
   */
  setMode(mode: LiveMode): void {
    if (this.mode === mode) return;
    const wasWarm = this.mode === "warm";
    this.mode = mode;
    this.root.dataset.live = mode;
    this.parts.chip.classList.toggle("live", mode === "live");
    if (mode === "off") {
      this.dropIframe();
      return;
    }
    const iframe = this.iframe ?? this.buildIframe();
    iframe.toggleAttribute("data-warm", mode === "warm");
    iframe.inert = mode === "warm";
    // Whatever the page did while it was hidden is not an owner scroll to follow.
    if (wasWarm && mode === "live") this.follower.reset();
    this.placeIframe();
  }

  private buildIframe(): HTMLIFrameElement {
    const iframe = el("iframe", {
      attrs: { title: `${this.project.name} ${this.frame.route}`, "data-frame": this.frame.id },
    });
    // A bridge starts with an empty watch list and no patch: both are sent again after every navigation
    // (the bridge's `pw:hello` does it too; a fresh document drops everything).
    iframe.addEventListener("load", () => {
      this.channel.loaded();
      this.follower.reset();
      this.sentPatch = "";
    });
    this.iframe = iframe;
    this.root.append(iframe);
    this.stats.built += 1;
    iframe.src = this.src;
    return iframe;
  }

  /** Removes the iframe; a stroke waiting for its hit falls back to an area binding. */
  dropIframe(): void {
    if (this.iframe === null) return;
    this.iframe.remove();
    this.iframe = null;
    this.window = null;
    this.sentPatch = "";
    this.stats.removed += 1;
    this.channel.close();
    this.channel.loaded();
    this.follower.reset();
  }

  /** The frame item is gone. */
  destroy(): void {
    this.dropIframe();
    this.root.remove();
    this.parts.chip.remove();
    this.channel.close();
  }

  /** The owner toggled the ghost: the patch is applied or reverted on the live page (local only). */
  setGhost(on: boolean): void {
    this.ghost = on;
    this.sendPatch(true);
  }

  /**
   * Sends the frame's patch (applied while the ghost is on, reverted while it is off) unless the current
   * document already has that exact state. A frame whose patch was cleared reverts what is still applied.
   */
  sendPatch(force = false): void {
    const send = patchFor(this.frame.patch, this.ghost);
    const key = send === null ? "none" : `${send.on ? "on" : "off"}:${send.patch}`;
    if (!force && key === this.sentPatch) return;
    const target = this.iframe?.contentWindow;
    if (!target) return;
    if (send !== null) target.postMessage(patchMessage(send.patch, send.on), "*");
    else if (this.sentPatch !== "" && this.sentPatch !== "none") {
      target.postMessage(patchMessage("", false), "*");
    }
    this.sentPatch = key;
  }

  /** A same-URL `src` assignment need not reload, so park on `about:blank` first. */
  private reload(): void {
    if (this.iframe === null) return;
    this.iframe.src = "about:blank";
    this.iframe.src = this.src;
  }
}
