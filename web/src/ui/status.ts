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

  show(state: ConnectionState, pending: number): void {
    this.node.dataset.state = state;
    const queued = pending > 0 ? ` · ${pending} unsent` : "";
    this.node.textContent = `${LABELS[state]}${queued}`;
  }

  problem(message: string): void {
    this.node.title = message;
    console.warn(message);
  }
}
