import { describe, expect, it } from "vitest";
import { OUTBOX_WAIT_MS, type OutboxClock, untilAcknowledged } from "./outbox";

function clock(): OutboxClock & { time: number } {
  const c = {
    time: 0,
    now: () => c.time,
    sleep: async (ms: number) => {
      c.time += ms;
    },
  };
  return c;
}

describe("untilAcknowledged", () => {
  it("returns at once when nothing is waiting", async () => {
    const c = clock();
    expect(await untilAcknowledged(() => 0, c)).toBe(true);
    expect(c.time).toBe(0);
  });

  it("waits for the acknowledgement, no longer", async () => {
    const c = clock();
    let pending = 3;
    const ok = await untilAcknowledged(() => (c.time >= 400 ? 0 : pending), c);
    pending = 0;
    expect(ok).toBe(true);
    expect(c.time).toBeGreaterThanOrEqual(400);
    expect(c.time).toBeLessThan(500);
  });

  it("gives up after 3 s and lets the caller send anyway", async () => {
    const c = clock();
    expect(await untilAcknowledged(() => 1, c)).toBe(false);
    expect(c.time).toBeGreaterThanOrEqual(OUTBOX_WAIT_MS);
    expect(c.time).toBeLessThan(OUTBOX_WAIT_MS + 100);
  });
});
