/**
 * Injected into the page: highlight elements under the cursor and report the clicked one.
 *
 * Runs in the isolated world, so it cannot clash with the page's globals. Reports through the extension
 * messaging channel because an injected file's return value is not awaited — the worker listens for `picked`.
 */

(() => {
  if (window.__penwisePickerActive) return;
  window.__penwisePickerActive = true;

  const outline = document.createElement("div");
  outline.style.cssText = [
    "position:fixed",
    "pointer-events:none",
    "z-index:2147483647",
    "border:2px solid #4f46e5",
    "background:rgba(79,70,229,0.08)",
  ].join(";");
  document.documentElement.append(outline);

  /** A selector that finds this element again: id, then a short ancestor path with nth-of-type. */
  function selectorFor(element) {
    if (element.id) return `#${CSS.escape(element.id)}`;
    const parts = [];
    let node = element;
    while (node && node.nodeType === Node.ELEMENT_NODE && parts.length < 4) {
      let part = node.tagName.toLowerCase();
      if (node.classList.length) part += `.${CSS.escape(node.classList[0])}`;
      const parent = node.parentElement;
      if (parent) {
        const siblings = [...parent.children].filter((child) => child.tagName === node.tagName);
        if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(node) + 1})`;
      }
      parts.unshift(part);
      const candidate = parts.join(" > ");
      if (document.querySelectorAll(candidate).length === 1) return candidate;
      node = parent;
    }
    return parts.join(" > ");
  }

  function rectOf(element) {
    const box = element.getBoundingClientRect();
    return [box.left, box.top, box.width, box.height];
  }

  function onMove(event) {
    const element = document.elementFromPoint(event.clientX, event.clientY);
    if (!element || element === outline) return;
    const [x, y, width, height] = rectOf(element);
    Object.assign(outline.style, {
      left: `${x}px`,
      top: `${y}px`,
      width: `${width}px`,
      height: `${height}px`,
    });
  }

  function finish(pick) {
    outline.remove();
    window.__penwisePickerActive = false;
    document.removeEventListener("mousemove", onMove, true);
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("keydown", onKey, true);
    chrome.runtime.sendMessage({ cmd: "picked", pick });
  }

  function onClick(event) {
    event.preventDefault();
    event.stopPropagation();
    const element = document.elementFromPoint(event.clientX, event.clientY);
    if (!element) return finish(null);
    finish({
      selector: selectorFor(element),
      rect: rectOf(element),
      role: element.getAttribute("role") ?? element.tagName.toLowerCase(),
      text: (element.textContent ?? "").trim().slice(0, 200),
    });
  }

  function onKey(event) {
    if (event.key === "Escape") finish(null);
  }

  document.addEventListener("mousemove", onMove, true);
  document.addEventListener("click", onClick, true);
  document.addEventListener("keydown", onKey, true);
})();
