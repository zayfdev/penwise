// Collects named checks (measured value + threshold + evidence), prints a table and writes the JSON report.
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const fmt = (v) => (v !== null && typeof v === 'object' ? JSON.stringify(v) : String(v));

export class Report {
  constructor(dir, meta) {
    this.dir = dir;
    this.meta = meta;
    this.results = [];
    this.notes = [];
    this.suffix = '';
  }

  /** status: pass | fail (an app/behaviour verdict) | error (the harness could not measure) | skip. */
  add(group, name, status, { measured = '', threshold = '', evidence = '' } = {}) {
    group += this.suffix;
    this.results.push({ group, name, status, measured, threshold, evidence });
    const mark = { pass: 'ok  ', fail: 'FAIL', error: 'ERR ', skip: 'skip', gap: 'GAP ' }[status];
    process.stderr.write(`  ${mark} ${group}/${name}  ${fmt(measured)}\n`);
  }

  check(group, name, pass, detail) {
    this.add(group, name, pass ? 'pass' : 'fail', detail);
    return pass;
  }

  /**
   * A check for behaviour the app does not have yet (a known gap, not an app regression and not a harness error):
   * it passes when the app has it, and reports `gap` (not counted as a failure) while it does not.
   */
  gap(group, name, pass, detail) {
    this.add(group, name, pass ? 'pass' : 'gap', { ...detail, evidence: `KNOWN GAP (expected to fail until the app change lands). ${detail.evidence ?? ''}` });
    return pass;
  }

  note(text) {
    this.notes.push(text);
  }

  get failures() {
    return this.results.filter((r) => r.status === 'fail' || r.status === 'error');
  }

  table() {
    const rows = this.results.map((r) => [r.group, r.name, r.status.toUpperCase(), fmt(r.measured), fmt(r.threshold)]);
    const head = ['group', 'check', 'verdict', 'measured', 'threshold'];
    const widths = head.map((h, i) => Math.min(70, Math.max(h.length, ...rows.map((r) => r[i].length))));
    const line = (cols) => cols.map((c, i) => c.slice(0, widths[i]).padEnd(widths[i])).join('  ');
    return [line(head), line(widths.map((w) => '-'.repeat(w))), ...rows.map(line)].join('\n');
  }

  async write() {
    await mkdir(this.dir, { recursive: true });
    const summary = {
      pass: this.results.filter((r) => r.status === 'pass').length,
      fail: this.results.filter((r) => r.status === 'fail').length,
      error: this.results.filter((r) => r.status === 'error').length,
      skip: this.results.filter((r) => r.status === 'skip').length,
      gap: this.results.filter((r) => r.status === 'gap').length,
    };
    const path = join(this.dir, 'report.json');
    await writeFile(path, JSON.stringify({ ...this.meta, summary, notes: this.notes, results: this.results }, null, 2));
    return path;
  }
}
