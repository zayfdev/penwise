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

/**
 * The elements worth attributing ink to: things with an id or test id, plus headings, sections and controls.
 * Rects come back in viewport CSS px; the caller scales them into screenshot pixels.
 */
async function collectAnchors(tabId) {
  const [{ result }] = await chrome.scripting.executeScript({
    target: { tabId },
    func: () => {
      const seen = new Set();
      const anchors = [];
      const candidates = document.querySelectorAll(
        "[id], [data-testid], h1, h2, h3, nav, section, article, button, a[href], form, img",
      );
      for (const element of candidates) {
        if (anchors.length >= 400) break;
        const box = element.getBoundingClientRect();
        if (box.width < 8 || box.height < 8) continue;
        if (box.bottom < 0 || box.top > innerHeight || box.right < 0 || box.left > innerWidth) continue;
        let selector;
        if (element.id) selector = `#${CSS.escape(element.id)}`;
        else if (element.dataset.testid) selector = `[data-testid="${element.dataset.testid}"]`;
        else continue;
        if (seen.has(selector)) continue;
        seen.add(selector);
        anchors.push({
          selector,
          rect: [box.left, box.top, box.width, box.height],
          role: element.getAttribute("role") ?? element.tagName.toLowerCase(),
          text: (element.textContent ?? "").trim().slice(0, 200),
        });
      }
      return anchors;
    },
  });
  return result ?? [];
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
  const dpr = await devicePixelRatio(tab.id);
  // The page is the screenshot, so an element's anchor rect is its CSS box times the pixel ratio.
  const anchors = (await collectAnchors(tab.id)).map((anchor) => ({
    ...anchor,
    rect: anchor.rect.map((value) => Math.round(value * dpr)),
    url: tab.url ?? "",
  }));
  return publishCapture(settings, {
    png,
    width: size.width,
    height: size.height,
    url: tab.url ?? "",
    title: tab.title ?? tab.url ?? "Capture",
    anchors,
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
        // The picture *is* the element, so its anchor covers the whole crop.
        rect: [0, 0, cropped.width, cropped.height],
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
