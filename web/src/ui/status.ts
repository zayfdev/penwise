/** Connection pill. */

import type { ConnectionState } from "../net/connection";

const LABELS: Record<ConnectionState, string> = {
  connecting: "connecting…",
  open: "live",
  closed: "offline — retrying",
  rejected: "access refused",
};

/** Shows the connection state and the latest problem. */
export class StatusPill {
  constructor(private readonly node: HTMLElement) {}

  show(state: ConnectionState, pending: number, refused = 0): void {
    this.node.dataset.state = state;
    const queued = pending > 0 ? ` · ${pending} unsent` : "";
    const dead = refused > 0 ? ` · ${refused} refused` : "";
    this.node.textContent = `${LABELS[state]}${queued}${dead}`;
  }

  problem(message: string): void {
    this.node.title = message;
    console.warn(message);
  }
}
