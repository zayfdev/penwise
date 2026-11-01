// Thin adb wrapper: shell commands, screenshots, input, UI dump, and rotation/wifi state.
import { execFile, spawn } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const ADB = process.env.ADB || join(homedir(), 'Android/Sdk/platform-tools/adb');
export const PKG = 'dev.penwise';

export function run(args, { input, timeout = 60000, binary = false } = {}) {
  return new Promise((resolve, reject) => {
    const child = execFile(
      ADB,
      args,
      { encoding: binary ? 'buffer' : 'utf8', maxBuffer: 256 * 1024 * 1024, timeout },
      (error, stdout, stderr) => {
        if (error) {
          error.stderr = String(stderr);
          reject(error);
        } else resolve(stdout);
      },
    );
    if (input !== undefined) child.stdin.end(input);
  });
}

export async function sh(command, opts) {
  const out = await run(['shell', command], opts);
  return out.replace(/\r/g, '');
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function screencap() {
  return run(['exec-out', 'screencap', '-p'], { binary: true, timeout: 30000 });
}

export async function pid() {
  const out = (await sh(`pidof ${PKG}`).catch(() => '')).trim();
  return out ? out.split(/\s+/)[0] : null;
}

/** Screen size in the current rotation: `wm size` reports the natural size, so swap on rotation 1/3. */
export async function screen() {
  const size = /(\d+)x(\d+)/.exec(await sh('wm size'));
  const rot = await rotation();
  const [w, h] = [Number(size[1]), Number(size[2])];
  return rot % 2 === 1 ? { w: h, h: w, rot } : { w, h, rot };
}

export async function rotation() {
  const out = await sh('dumpsys window displays');
  const m = /mCurrentRotation=ROTATION_(\d+)/.exec(out);
  return m ? Number(m[1]) / 90 : 0;
}

export async function snapshotDeviceState() {
  const get = async (k) => (await sh(`settings get system ${k}`)).trim();
  const wifi = /Wi-Fi is enabled/.test(await sh('dumpsys wifi'));
  return { accel: await get('accelerometer_rotation'), user: await get('user_rotation'), wifi };
}

export async function setRotation(rot) {
  await sh(`settings put system accelerometer_rotation 0; settings put system user_rotation ${rot}`);
}

export async function setWifi(on) {
  await sh(`svc wifi ${on ? 'enable' : 'disable'}`);
}

export async function restoreDeviceState(state) {
  await sh(`settings put system user_rotation ${state.user}; settings put system accelerometer_rotation ${state.accel}`).catch(() => {});
  await setWifi(state.wifi).catch(() => {});
}

export async function wake() {
  await sh('input keyevent KEYCODE_WAKEUP; wm dismiss-keyguard; settings put global stay_on_while_plugged_in 3').catch(() => {});
}

export const tap = (x, y) => sh(`input tap ${Math.round(x)} ${Math.round(y)}`);
export const doubleTap = (x, y) => {
  const [a, b] = [Math.round(x), Math.round(y)];
  return sh(`input tap ${a} ${b}; input tap ${a} ${b}`);
};
const r = (n) => Math.round(n);
export const finger = (x1, y1, x2, y2, ms = 400) =>
  sh(`input swipe ${r(x1)} ${r(y1)} ${r(x2)} ${r(y2)} ${r(ms)}`);
export const pen = (x1, y1, x2, y2, ms = 400) =>
  sh(`input stylus swipe ${r(x1)} ${r(y1)} ${r(x2)} ${r(y2)} ${r(ms)}`);

/** `uiautomator dump` parsed to flat nodes {text, desc, id, bounds:[x1,y1,x2,y2], clickable}. */
export async function uiNodes() {
  await sh('uiautomator dump /sdcard/penwise-qa-ui.xml');
  const xml = await sh('cat /sdcard/penwise-qa-ui.xml');
  const nodes = [];
  for (const m of xml.matchAll(/<node ([^>]*?)\/?>/g)) {
    const attr = (n) => new RegExp(`${n}="([^"]*)"`).exec(m[1])?.[1] ?? '';
    const b = /\[(\d+),(\d+)\]\[(\d+),(\d+)\]/.exec(attr('bounds'));
    if (!b) continue;
    nodes.push({
      text: attr('text'),
      desc: attr('content-desc'),
      id: attr('resource-id'),
      pkg: attr('package'),
      clickable: attr('clickable') === 'true',
      bounds: b.slice(1).map(Number),
    });
  }
  return nodes;
}

export const center = (n) => [(n.bounds[0] + n.bounds[2]) / 2, (n.bounds[1] + n.bounds[3]) / 2];

export async function startApp() {
  await sh(`am start -n ${PKG}/.MainActivity`);
}
export const forceStop = () => sh(`am force-stop ${PKG}`);
export const home = () => sh('input keyevent KEYCODE_HOME');

export function logcatProcess(args) {
  return spawn(ADB, ['logcat', ...args]);
}
