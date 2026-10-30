/**
 * Camera moves another device asks for (`SwitchPage`, `Focus`) while the owner is drawing here: they wait
 * behind a chip instead of yanking the page away (design-harness v2 contract §3). Pure.
 */

import type { MoveKind } from "./follow";

/** The owner counts as busy this long after the last stroke, ms. */
export const AGENT_DEFER_MS = 10_000;

/**
 * The devices that are the owner's hands: any device seen inking live (a tablet pen, another browser). The client
 * cannot ask the server which device is the agent, so "not a hand" stands for it: a move from a hand is the owner
 * moving their own camera, a move from anything else is the agent's.
 */
export class OwnerDevices {
  private readonly seen = new Set<string>();

  /** `device` produced live ink. */
  inked(device: string): void {
    this.seen.add(device);
  }

  /** True when `device` has inked in this session. */
  has(device: string | null): boolean {
    return device !== null && this.seen.has(device);
  }
}

/**
 * Whether a move of `kind` waits for the owner: only moves the agent chose (not the owner's own tablet, not a
 * reconnect's `Welcome`), and only while a stroke is open or the last one ended less than {@link AGENT_DEFER_MS}
 * ago.
 */
export function shouldDefer(
  kind: MoveKind,
  fromOwner: boolean,
  lastInkAt: number | null,
  now: number,
  drawing: boolean,
): boolean {
  if (kind === "welcome" || fromOwner) return false;
  return drawing || (lastInkAt !== null && now - lastInkAt < AGENT_DEFER_MS);
}

/** What the chip names: the frame's label (or route), else the page's title, else "page". */
export function moveLabel(
  item: string | null,
  frames: readonly { readonly id: string; readonly label: string; readonly route: string }[],
  pageTitle: string | null,
): string {
  const frame = item === null ? undefined : frames.find((f) => f.id === item);
  if (frame !== undefined) return frame.label !== "" ? frame.label : frame.route;
  return pageTitle !== null && pageTitle !== "" ? pageTitle : "page";
}

/** The chip text. */
export function chipText(label: string): string {
  return `Agent: open ${label} →`;
}
