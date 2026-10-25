/**
 * Service worker: turns the active tab into a Penwise page.
 *
 * The popup asks for one of two things; the work happens here because `chrome.tabs.captureVisibleTab` and
 * script injection are only available to the extension, and a popup closes as soon as it loses focus.
 */

import { cropToElement, loadSettings, publishCapture } from "./server.js";

/** Resolves with the element the user clicks, or `null` after Escape. */
function waitForPick(tabId) {
  return new Promise((resolve) => {
    const timeout = setTimeout(() => finish(null), PICK_TIMEOUT_MS);
    function onMessage(message, sender) {
      if (message?.cmd !== "picked" || sender.tab?.id !== tabId) return;
      finish(message.pick ?? null);
    }
    function finish(value) {
      clearTimeout(timeout);
      chrome.runtime.onMessage.removeListener(onMessage);
      resolve(value);
    }
    chrome.runtime.onMessage.addListener(onMessage);
    chrome.scripting
      .executeScript({ target: { tabId }, files: ["picker.js"] })
      .catch(() => finish(null));
  });
}

/** Screenshots the visible viewport as a PNG Blob. */
async function captureViewport(windowId) {
  const dataUrl = await chrome.tabs.captureVisibleTab(windowId, { format: "png" });
  return await (await fetch(dataUrl)).blob();
}

/** The device pixel ratio of the tab, so CSS-pixel rects map onto screenshot pixels. */
async function devicePixelRatio(tabId) {
  const [{ result }] = await chrome.scripting.executeScript({
    target: { tabId },
    func: () => window.devicePixelRatio || 1,
  });
  return result || 1;
}

/** Captures the whole visible tab and publishes it as a page. */
async function captureTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) throw new Error("no active tab");
  const settings = await loadSettings();
  const png = await captureViewport(tab.windowId);
  const size = await createImageBitmap(png);
  return publishCapture(settings, {
    png,
    width: size.width,
    height: size.height,
    url: tab.url ?? "",
    title: tab.title ?? tab.url ?? "Capture",
  });
}

/** Lets the user click one element; captures just that element and its selector. */
async function captureElement() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) throw new Error("no active tab");
  const pick = await waitForPick(tab.id);
  if (!pick) throw new Error("nothing picked");
  const settings = await loadSettings();
  const png = await captureViewport(tab.windowId);
  const dpr = await devicePixelRatio(tab.id);
  const cropped = await cropToElement({ png, rect: pick.rect, dpr });
  return publishCapture(settings, {
    png: cropped.png,
    width: cropped.width,
    height: cropped.height,
    url: tab.url ?? "",
    title: `${pick.selector} — ${tab.title ?? ""}`.trim(),
    anchors: [
      {
        selector: pick.selector,
        rect: pick.rect,
        role: pick.role,
        text: pick.text,
        url: tab.url ?? "",
      },
    ],
  });
}

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.cmd === "picked") return false;
  const run = message?.cmd === "capture-element" ? captureElement : captureTab;
  run()
    .then((result) => respond({ ok: true, result }))
    .catch((error) => respond({ ok: false, error: String(error?.message ?? error) }));
  return true;
});

const PICK_TIMEOUT_MS = 120_000;
