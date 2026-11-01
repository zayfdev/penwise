// Minimal MCP stdio client for ~/.local/bin/penwise-mcp: one process per call batch, newline-delimited JSON-RPC.
import { spawn } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const BIN = process.env.PENWISE_MCP || join(homedir(), '.local/bin/penwise-mcp');

export class Mcp {
  constructor({ port, project }) {
    this.child = spawn(BIN, [], {
      env: { ...process.env, PENWISE_PORT: String(port), PENWISE_PROJECT: project },
      stdio: ['pipe', 'pipe', 'inherit'],
    });
    this.next = 1;
    this.waiting = new Map();
    let buf = '';
    this.child.stdout.on('data', (chunk) => {
      buf += chunk;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i).trim();
        buf = buf.slice(i + 1);
        if (!line) continue;
        const msg = JSON.parse(line);
        this.waiting.get(msg.id)?.(msg);
        this.waiting.delete(msg.id);
      }
    });
  }

  rpc(method, params, timeout = 190000) {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`mcp ${method} timed out`)), timeout);
      this.waiting.set(id, (msg) => {
        clearTimeout(timer);
        resolve(msg);
      });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    });
  }

  async init() {
    await this.rpc('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'device-qa', version: '1' },
    });
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
    return this;
  }

  /** Calls a tool; returns {text, isError, json} (json when the text parses). */
  async call(name, args = {}) {
    const msg = await this.rpc('tools/call', { name, arguments: args });
    if (msg.error) throw new Error(`mcp ${name}: ${JSON.stringify(msg.error)}`);
    const text = (msg.result.content ?? []).filter((c) => c.type === 'text').map((c) => c.text).join('\n');
    let json;
    try {
      json = JSON.parse(text);
    } catch {}
    return { text, isError: !!msg.result.isError, json };
  }

  close() {
    this.child.stdin.end();
    this.child.kill();
  }
}
