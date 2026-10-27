import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { PaperGrain } from "./grain";
import { coverageAt, type Dab, falloff, minFade, pixelDab } from "./stamps";

/**
 * Parity with core: `crates/penwise-core/tests/goldens/stamp_coverage.txt` holds the coverage `S` core's
 * `render_stamps` leaves at sample pixels (format in `tests/stamp_parity.rs`). The browser's port must reproduce it.
 */
const FIXTURE = new URL(
  "../../../crates/penwise-core/tests/goldens/stamp_coverage.txt",
  import.meta.url,
);

interface Sample {
  col: number;
  row: number;
  height: number;
  s: number;
}
interface Case {
  name: string;
  scale: number;
  originX: number;
  originY: number;
  grainScale: number;
  softness: number;
  dabs: Dab[];
  samples: Sample[];
}
interface HeightRow {
  x: number;
  y: number;
  scale: number;
  value: number;
}

function parse(text: string): { cases: Case[]; heights: HeightRow[] } {
  const cases: Case[] = [];
  const heights: HeightRow[] = [];
  for (const line of text.split("\n")) {
    const [tag, ...rest] = line.trim().split(/\s+/);
    if (!tag || tag.startsWith("#")) continue;
    const n = rest.map(Number);
    const [a, b, c, d, e, f, g, h, i] = n;
    if (tag === "case") {
      cases.push({
        name: rest[0] ?? "",
        scale: Number(rest[1]),
        originX: Number(rest[2]),
        originY: Number(rest[3]),
        grainScale: Number(rest[4]),
        softness: Number(rest[5]),
        dabs: [],
        samples: [],
      });
    } else if (tag === "dab") {
      cases.at(-1)?.dabs.push({
        x: a ?? 0,
        y: b ?? 0,
        radiusX: c ?? 0,
        radiusY: d ?? 0,
        angle: e ?? 0,
        opacity: f ?? 0,
        hardness: g ?? 0,
        grainStrength: h ?? 0,
        grainLevel: i ?? 0,
      });
    } else if (tag === "at") {
      cases.at(-1)?.samples.push({ col: a ?? 0, row: b ?? 0, height: c ?? 0, s: d ?? 0 });
    } else if (tag === "height") {
      heights.push({ x: a ?? 0, y: b ?? 0, scale: c ?? 0, value: d ?? 0 });
    }
  }
  return { cases, heights };
}

const { cases, heights } = parse(readFileSync(FIXTURE, "utf8"));

describe("stamp coverage parity with core", () => {
  it("loads the fixture", () => {
    expect(cases.length).toBeGreaterThanOrEqual(5);
    expect(heights.length).toBeGreaterThan(50);
  });

  it("paper height is core's paper_height", () => {
    const grain = new PaperGrain();
    let worst = 0;
    for (const row of heights) {
      worst = Math.max(worst, Math.abs(grain.height(row.x, row.y, row.scale) - row.value));
    }
    expect(worst).toBeLessThan(2e-4);
  });

  for (const c of cases) {
    it(`${c.name}: coverage at ${c.samples.length} sample pixels`, () => {
      const view = { scale: c.scale, originX: c.originX, originY: c.originY };
      const dabs = c.dabs.flatMap((d) => pixelDab(d, view) ?? []);
      let worst = 0;
      for (const sample of c.samples) {
        // The fixture's height is core's value at this pixel; it isolates the formulas from the noise port.
        const grain =
          c.grainScale > 0
            ? {
                style: { scale: c.grainScale, softness: c.softness },
                height: () => sample.height,
                view,
              }
            : null;
        const got = coverageAt(dabs, sample.col, sample.row, grain);
        worst = Math.max(worst, Math.abs(got - sample.s));
      }
      expect(worst).toBeLessThan(1e-4);
    });
  }

  it("matches end to end with the ported noise within 5e-4", () => {
    const paper = new PaperGrain();
    for (const c of cases.filter((k) => k.grainScale > 0)) {
      const view = { scale: c.scale, originX: c.originX, originY: c.originY };
      const dabs = c.dabs.flatMap((d) => pixelDab(d, view) ?? []);
      const grain = {
        style: { scale: c.grainScale, softness: c.softness },
        height: (x: number, y: number) => paper.height(x, y, c.grainScale),
        view,
      };
      let worst = 0;
      for (const s of c.samples) {
        worst = Math.max(worst, Math.abs(coverageAt(dabs, s.col, s.row, grain) - s.s));
      }
      expect(worst).toBeLessThan(5e-4);
    }
  });
});

describe("core formulas", () => {
  it("falloff is the MyPaint two-segment curve", () => {
    expect(falloff(0, 0.5)).toBe(1);
    expect(falloff(1.01, 0.5)).toBe(0);
    expect(falloff(0.99, 1)).toBe(1);
    expect(falloff(0.5, 0)).toBe(0);
    expect(falloff(0.5, 0.5)).toBeCloseTo(0.5);
    expect(falloff(0.75, 0.5)).toBeCloseTo(0.25);
  });

  it("minFade re-solves thin dabs and floors sub-pixel ones at hardness 0.1", () => {
    expect(minFade(10, 0.5)).toEqual([10, 0.5, false]);
    const [r, h, faded] = minFade(0.4, 0.5);
    expect(faded).toBe(true);
    expect(h).toBeCloseTo(0.1);
    expect(r).toBeCloseTo(1 / 0.9);
  });

  it("a sub-pixel dab stays visible (core min_fade) instead of vanishing", () => {
    const view = { scale: 1, originX: 0, originY: 0 };
    const dab: Dab = {
      x: 10.5,
      y: 10.5,
      radiusX: 0.3,
      radiusY: 0.3,
      angle: 0,
      opacity: 1,
      hardness: 0.5,
      grainStrength: 0,
      grainLevel: 1,
    };
    const pd = pixelDab(dab, view);
    expect(pd).not.toBeNull();
    expect(coverageAt(pd ? [pd] : [], 10, 10, null)).toBeGreaterThan(0.5);
  });
});
