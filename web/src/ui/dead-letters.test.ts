import { describe, expect, it } from "vitest";
import { exportDeadLetters } from "./dead-letters";

describe("exportDeadLetters", () => {
  it("writes every dead letter with its envelope as base64", () => {
    const source = {
      dead_letters: () => [
        { op_id: "A", reason: "invalid op", summary: "item edit on page P" },
        { op_id: "B", reason: "unknown page", summary: "page change Q" },
      ],
      dead_letter_bytes: (id: string) => (id === "A" ? new Uint8Array([1, 2, 3]) : undefined),
    };
    const parsed = JSON.parse(exportDeadLetters(source as never)) as unknown[];
    expect(parsed).toEqual([
      { op_id: "A", reason: "invalid op", summary: "item edit on page P", envelope_base64: "AQID" },
      { op_id: "B", reason: "unknown page", summary: "page change Q", envelope_base64: "" },
    ]);
  });
});
