/** Waiting for the local outbox: "Send" must not overtake the strokes the owner drew just before pressing it. */

/** How long Send waits for the server to acknowledge the outbox, ms (contract §7). */
export const OUTBOX_WAIT_MS = 3_000;

/** Time, as Send needs it (injected for tests). */
export interface OutboxClock {
  now(): number;
  sleep(ms: number): Promise<void>;
}

const STEP_MS = 50;

/**
 * Resolves `true` once `pending()` (ops queued or sent but not acknowledged) is zero, `false` when it is still not
 * after `timeoutMs` — the caller then sends anyway.
 */
export async function untilAcknowledged(
  pending: () => number,
  clock: OutboxClock,
  timeoutMs: number = OUTBOX_WAIT_MS,
): Promise<boolean> {
  const deadline = clock.now() + timeoutMs;
  while (pending() > 0) {
    if (clock.now() >= deadline) return false;
    await clock.sleep(STEP_MS);
  }
  return true;
}

/** {@link untilAcknowledged} on the real clock. */
export function untilAcknowledgedNow(pending: () => number): Promise<boolean> {
  return untilAcknowledged(pending, {
    now: () => Date.now(),
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  });
}
