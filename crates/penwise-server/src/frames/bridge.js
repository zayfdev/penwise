/*
 * Penwise bridge — injected into every HTML page served by a project port.
 *
 * Contract (docs/design-harness-spec.md §6.2, C3):
 *   out: {t:"pw:hello", url, docW, docH, vw, vh}              on load
 *        {t:"pw:scroll", x, y, docW, docH}                     one per frame (rAF, timer fallback)
 *        {t:"pw:layout", scroll, doc, viewport, state, rects}  the element layout marks are drawn against
 *        {t:"pw:hit", id, key, selector, rect}                 in answer to a hit request
 *        {t:"pw:route", route}                                 when the location changes
 *        {t:"pw:replayed", state, ok}                          in answer to a replay request
 *        {t:"pw:patched", ok, failed:[{index, reason}]}        after a patch apply/revert
 *   in:  {t:"pw:hit?", id, x, y}                               x/y are viewport CSS px
 *        {t:"pw:watch", keys:[...]}                            replace the watched element keys
 *        {t:"pw:replay", state}                                replay a recorded click recipe
 *        {t:"pw:patch", patch, on}                             on=true applies, on=false reverts
 *        {t:"pw:scrollTo", x, y}
 *        {t:"pw:navigate", route}                              same-origin path; a full navigation
 *
 * `pw:layout` fields are viewport CSS px (unscaled, unrounded): `scroll` and `doc` in document coordinates,
 * `viewport` in CSS px, `rects` a map of element key → `[x,y,w,h]` relative to the viewport, or `null` when the
 * element is gone. The first message (and the first one after a new page) is full; later ones carry only the
 * rects whose value changed, plus every other field in full. A key removed from `pw:watch` is simply absent, so
 * `null` always means "the element was there and is gone". Keys are `v1|<data-pw-src>|<fingerprint>`:
 * tag, id, stable classes and the nth-of-type path from the nearest ancestor with an id.
 *
 * The state recipe is a `;`-joined list of `click:<selector>` steps: clicks that toggled `aria-expanded`, a
 * `details`/`dialog` open state, or a state class on the target or the element it controls with `aria-controls`.
 * A step disappears again when its toggle returns to the value it had before, so closing the menu resets the
 * recipe to "". Replay clicks the recorded selectors in order on the current route and reports whether the page
 * ended up in that state.
 *
 * A patch (`pw:patch`) is `{css, dom:[{op, selector, html?, name?, value?}]}`: the CSS goes into one
 * `<style data-pw-patch>`, the ops run against every element their selector matches, and unmatched or invalid
 * selectors come back in `failed` with their index. `on=false` restores the pre-patch DOM exactly (placeholders
 * for removed elements, recorded attributes/classes/text). The patch is not stored: the host re-sends it after
 * every `pw:hello`/load, and the layout and keys keep working on the patched DOM.
 *
 * The scroll position is saved per `location.pathname` in session storage before a reload/`pagehide` and restored
 * after load, so the owner stays where they were when the code changes.
 *
 * The host is the PC window (window.parent, an iframe) or the Android WebView (window.PenwiseHost.post).
 * Static projects also open /__penwise/reload and reload on {t:"pw:reload"}; proxied projects get HMR from
 * their own dev server.
 *
 * Dependency-free and defensive: every entry point swallows its errors so a broken bridge can never break the
 * page it was injected into.
 */
(function () {
  "use strict";

  var HOST_JS = "PenwiseHost";
  var RELOAD_PATH = "/__penwise/reload";
  var SCROLL_PREFIX = "pw:scroll:";
  var KEY_VERSION = "v1";
  var MAX_WATCHED = 2048;
  var parent = window.parent && window.parent !== window ? window.parent : null;
  var host = window[HOST_JS] || null;
  var lastScroll = "";
  var lastHello = "";
  var lastRoute = "";

  var watchKeys = [];
  var lastRects = {};
  var lastFields = "";
  var haveLayout = false;
  var observed = {};
  var resizeObserver = null;

  var state = "";
  var stateSteps = [];

  var PATCH_ATTR = "data-pw-patch";
  var patchUndo = [];
  var patchActive = false;

  function send(message) {
    try {
      if (host && typeof host.post === "function") {
        host.post(JSON.stringify(message));
      } else if (parent) {
        parent.postMessage(message, "*");
      } else {
        // A top-level page (a debug window) has no host: report to itself so the messages stay observable.
        window.postMessage(message, "*");
      }
    } catch (_) {
      /* the host went away */
    }
  }

  function docSize() {
    var root = document.documentElement || document.body;
    var body = document.body;
    return {
      w: Math.max(root.scrollWidth, root.clientWidth, body ? body.scrollWidth : 0),
      h: Math.max(root.scrollHeight, root.clientHeight, body ? body.scrollHeight : 0)
    };
  }

  function scrollPos() {
    var root = document.documentElement;
    return {
      x: window.pageXOffset || (root && root.scrollLeft) || 0,
      y: window.pageYOffset || (root && root.scrollTop) || 0
    };
  }

  function storage() {
    try {
      return window.sessionStorage || null;
    } catch (_) {
      // Blocked storage (a strict third-party context) must not break the page.
      return null;
    }
  }

  // One entry per route, so a reload keeps the owner where they were and a route switch keeps its own place.
  function scrollKey() {
    return SCROLL_PREFIX + location.pathname;
  }

  function saveScroll() {
    var store = storage();
    if (!store) return;
    var pos = scrollPos();
    try {
      store.setItem(scrollKey(), pos.x + "," + pos.y);
    } catch (_) {
      /* quota or a blocked store */
    }
  }

  function restoreScroll() {
    var store = storage();
    if (!store) return;
    var value = null;
    try {
      value = store.getItem(scrollKey());
    } catch (_) {
      return;
    }
    if (!value) return;
    var parts = value.split(",");
    var x = Number(parts[0]);
    var y = Number(parts[1]);
    if (!isFinite(x) || !isFinite(y)) return;
    var current = scrollPos();
    // The browser may already have restored the position (bfcache); do not fight it.
    if (current.x > 1 || current.y > 1) return;
    window.scrollTo(x, y);
  }

  function route() {
    return location.pathname + location.search + location.hash;
  }

  // `{t:"pw:route"}` whenever the location changes under the page (SPA routing, back/forward, hash). The state
  // recipe is "since the last navigation", so it starts over here.
  function reportRoute() {
    var now = route();
    if (now === lastRoute) return;
    lastRoute = now;
    state = "";
    stateSteps = [];
    send({ t: "pw:route", route: now });
    scheduleLayout();
  }

  function hookHistory() {
    var history = window.history;
    if (!history || history.__penwiseRouteHook) return;
    history.__penwiseRouteHook = true;
    var names = ["pushState", "replaceState"];
    for (var i = 0; i < names.length; i += 1) {
      var original = history[names[i]];
      if (typeof original !== "function") continue;
      history[names[i]] = (function (method) {
        return function () {
          var result = method.apply(this, arguments);
          reportRoute();
          return result;
        };
      })(original);
    }
    window.addEventListener("popstate", reportRoute, false);
    window.addEventListener("hashchange", reportRoute, false);
  }

  function hello() {
    var size = docSize();
    var message = {
      t: "pw:hello",
      url: location.href,
      docW: size.w,
      docH: size.h,
      vw: window.innerWidth,
      vh: window.innerHeight
    };
    // DOMContentLoaded and load both fire; send one hello unless the document actually changed.
    var key = JSON.stringify(message);
    if (key === lastHello) return;
    lastHello = key;
    send(message);
  }

  function scrollNow() {
    var pos = scrollPos();
    var size = docSize();
    var key = pos.x + "," + pos.y + "," + size.w + "," + size.h;
    if (key === lastScroll) return;
    lastScroll = key;
    send({ t: "pw:scroll", x: pos.x, y: pos.y, docW: size.w, docH: size.h });
  }

  // rAF is throttled away in a hidden tab or a background WebView; a timer keeps both streams alive there.
  // Whichever fires first wins, and the pending flag coalesces a burst into one run.
  function scheduler(run) {
    var pending = false;
    var timer = 0;
    var frame = 0;
    function flush() {
      if (!pending) return;
      pending = false;
      if (timer) {
        window.clearTimeout(timer);
        timer = 0;
      }
      if (frame && window.cancelAnimationFrame) {
        window.cancelAnimationFrame(frame);
        frame = 0;
      }
      run();
    }
    return function schedule() {
      if (pending) return;
      pending = true;
      timer = window.setTimeout(flush, 64);
      frame = window.requestAnimationFrame ? window.requestAnimationFrame(flush) : 0;
    };
  }

  var scheduleScroll = scheduler(scrollNow);

  /* ------------------------------------------------------------------ element keys */

  var STATE_CLASSES = {
    open: 1,
    opened: 1,
    closed: 1,
    active: 1,
    inactive: 1,
    selected: 1,
    expanded: 1,
    collapsed: 1,
    show: 1,
    shown: 1,
    visible: 1,
    hidden: 1,
    on: 1,
    off: 1,
    current: 1,
    checked: 1,
    pressed: 1,
    toggled: 1
  };

  var STATE_SUFFIX = /-(open|opened|closed|active|expanded|collapsed|visible|hidden|shown|selected)$/;

  function stateClass(name) {
    return STATE_CLASSES[name] === 1 || /^(is-|has-|js-)/.test(name) || STATE_SUFFIX.test(name);
  }

  // Build-time and CSS-in-JS class names change between builds; they would make a key churn.
  function hashClass(name) {
    if (/^(css|sc|emotion|tw|svelte|jsx)-/.test(name)) return true;
    if (name.length >= 8 && /[0-9]/.test(name) && /[a-z]/i.test(name) && name.indexOf("-") < 0) {
      return true;
    }
    return false;
  }

  function nthOfType(element) {
    var index = 1;
    for (var sibling = element.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
      if (sibling.tagName === element.tagName) index += 1;
    }
    return index;
  }

  function cssEscape(text) {
    if (window.CSS && window.CSS.escape) return window.CSS.escape(text);
    return String(text).replace(/[^a-zA-Z0-9_-]/g, "\\$&");
  }

  // One fingerprint segment: tag, id, stable classes, nth-of-type. No '|' and no '>' inside a segment.
  function segmentOf(element) {
    var text = element.tagName.toLowerCase();
    if (element.id) text += "#" + element.id;
    var list = element.classList;
    var classes = [];
    if (list) {
      for (var i = 0; i < list.length; i += 1) {
        var name = list[i];
        if (stateClass(name) || hashClass(name)) continue;
        classes.push(name);
      }
    }
    if (classes.length) text += "." + classes.join(".");
    return text + ":nth-of-type(" + nthOfType(element) + ")";
  }

  // tag + id + stable classes + the nth-of-type path from the nearest ancestor with an id (inclusive), so a
  // re-render that recreates the nodes keeps the same fingerprint.
  function fingerprintOf(element) {
    var parts = [];
    for (var node = element; node && node.nodeType === 1; node = node.parentElement) {
      parts.unshift(segmentOf(node));
      if (node.id) break;
    }
    return parts.join(" > ");
  }

  // `v1|<data-pw-src>|<fingerprint>`: the stable key bound marks use.
  function keyOf(element) {
    var src = "";
    try {
      src = element.getAttribute("data-pw-src") || "";
    } catch (_) {
      src = "";
    }
    return KEY_VERSION + "|" + src + "|" + fingerprintOf(element);
  }

  var SEGMENT = /^([a-z0-9-]+)(?:#([^.:>]+))?(?:\.([^:>]+))?:nth-of-type\((\d+)\)$/i;

  function parseSegment(text) {
    var match = SEGMENT.exec(text || "");
    if (!match) return null;
    return {
      tag: match[1].toLowerCase(),
      id: match[2] || "",
      nth: Number(match[4])
    };
  }

  // The `nth`-th direct child of `parent` with the segment's tag.
  function matchChild(parentNode, segment) {
    if (!parentNode || !segment) return null;
    var index = 0;
    for (var child = parentNode.firstElementChild; child; child = child.nextElementSibling) {
      if (child.tagName.toLowerCase() !== segment.tag) continue;
      index += 1;
      if (index === segment.nth) return child;
    }
    return null;
  }

  // A source attribute can repeat (one component, several instances); among the matches, the fingerprint picks
  // the instance the mark was drawn on, and the first match is the fallback the contract asks for.
  function bySource(src, fingerprint) {
    var candidates = document.querySelectorAll("[data-pw-src]");
    var first = null;
    for (var i = 0; i < candidates.length; i += 1) {
      if (candidates[i].getAttribute("data-pw-src") !== src) continue;
      if (fingerprintOf(candidates[i]) === fingerprint) return candidates[i];
      if (!first) first = candidates[i];
    }
    return first;
  }

  function byIdPath(segments) {
    var node = null;
    if (segments[0].id) {
      node = document.getElementById(segments[0].id);
      if (node && node.tagName.toLowerCase() !== segments[0].tag) node = null;
    } else if (document.documentElement && document.documentElement.tagName.toLowerCase() === segments[0].tag) {
      node = document.documentElement;
    }
    if (!node) return null;
    for (var i = 1; i < segments.length; i += 1) {
      node = matchChild(node, segments[i]);
      if (!node) return null;
    }
    return node;
  }

  function byFingerprint(segments, fingerprint) {
    var candidates = document.getElementsByTagName(segments[0].tag);
    for (var i = 0; i < candidates.length; i += 1) {
      if (fingerprintOf(candidates[i]) === fingerprint) return candidates[i];
    }
    return null;
  }

  // Resolution order of the contract: exact `data-pw-src`, then the id-anchored nth-of-type path, then a
  // fingerprint scan, then null.
  function resolveKey(key) {
    if (typeof key !== "string") return null;
    var first = key.indexOf("|");
    var second = first < 0 ? -1 : key.indexOf("|", first + 1);
    if (first < 0 || second < 0) return null;
    if (key.slice(0, first) !== KEY_VERSION) return null;
    var src = key.slice(first + 1, second);
    var fingerprint = key.slice(second + 1);
    if (src) {
      var exact = bySource(src, fingerprint);
      if (exact) return exact;
    }
    var texts = fingerprint.split(" > ");
    var segments = [];
    for (var i = 0; i < texts.length; i += 1) {
      var segment = parseSegment(texts[i]);
      if (!segment) return null;
      segments.push(segment);
    }
    if (!segments.length) return null;
    var node = byIdPath(segments);
    if (node) return node;
    return byFingerprint(segments, fingerprint);
  }

  /* ------------------------------------------------------------------ layout */

  function rectOf(element) {
    var rect = element.getBoundingClientRect();
    return [rect.left, rect.top, rect.width, rect.height];
  }

  function observeElements(entries) {
    if (!window.ResizeObserver) return;
    if (!resizeObserver) {
      try {
        resizeObserver = new window.ResizeObserver(function () {
          scheduleLayout();
        });
      } catch (_) {
        return;
      }
    }
    var next = {};
    for (var i = 0; i < entries.length; i += 1) {
      if (entries[i][1]) next[entries[i][0]] = entries[i][1];
    }
    for (var key in observed) {
      if (!Object.prototype.hasOwnProperty.call(observed, key)) continue;
      if (observed[key] !== next[key]) {
        try {
          resizeObserver.unobserve(observed[key]);
        } catch (_) {
          /* already gone */
        }
      }
    }
    for (var fresh in next) {
      if (!Object.prototype.hasOwnProperty.call(next, fresh)) continue;
      if (!observed[fresh] || observed[fresh] !== next[fresh]) {
        try {
          resizeObserver.observe(next[fresh]);
        } catch (_) {
          /* not an element */
        }
      }
    }
    observed = next;
  }

  function layoutNow() {
    var size = docSize();
    var pos = scrollPos();
    var viewport = [window.innerWidth, window.innerHeight];
    var rects = {};
    var all = {};
    var entries = [];
    var changed = false;
    for (var i = 0; i < watchKeys.length; i += 1) {
      var key = watchKeys[i];
      var element = resolveKey(key);
      entries.push([key, element]);
      var rect = element ? rectOf(element) : null;
      all[key] = rect;
      var text = JSON.stringify(rect);
      if (lastRects[key] !== text) {
        lastRects[key] = text;
        rects[key] = rect;
        changed = true;
      }
    }
    observeElements(entries);
    if (!haveLayout) {
      haveLayout = true;
      rects = all;
      changed = true;
    }
    var fields = JSON.stringify({
      scroll: [pos.x, pos.y],
      doc: [size.w, size.h],
      viewport: viewport,
      state: state
    });
    if (!changed && fields === lastFields) return;
    lastFields = fields;
    send({
      t: "pw:layout",
      scroll: [pos.x, pos.y],
      doc: [size.w, size.h],
      viewport: viewport,
      state: state,
      rects: rects
    });
  }

  var scheduleLayout = scheduler(layoutNow);

  // Replace the watched keys and force one layout message: a newly watched key is always carried, and a
  // vanished one is reported as `null` on the next frame.
  function watch(keys) {
    if (!keys || typeof keys.length !== "number") return;
    var next = [];
    var seen = {};
    for (var i = 0; i < keys.length && next.length < MAX_WATCHED; i += 1) {
      var key = keys[i];
      if (typeof key !== "string" || !key || seen[key]) continue;
      seen[key] = true;
      next.push(key);
    }
    watchKeys = next;
    scheduleLayout();
  }

  function observeDom() {
    if (!window.MutationObserver || !document.documentElement) return;
    try {
      new window.MutationObserver(function () {
        scheduleLayout();
      }).observe(document.documentElement, {
        subtree: true,
        childList: true,
        attributes: true,
        characterData: true
      });
    } catch (_) {
      /* no observer support */
    }
  }

  /* ------------------------------------------------------------------ state recipe */

  function controlledElements(element) {
    var found = [];
    var ids = element.getAttribute ? element.getAttribute("aria-controls") : null;
    if (!ids) return found;
    var parts = String(ids).split(/\s+/);
    for (var i = 0; i < parts.length; i += 1) {
      if (!parts[i]) continue;
      var node = document.getElementById(parts[i]);
      if (node) found.push(node);
    }
    return found;
  }

  function classState(element) {
    var list = element.classList;
    var names = [];
    if (list) {
      for (var i = 0; i < list.length; i += 1) {
        if (stateClass(list[i])) names.push(list[i]);
      }
    }
    names.sort();
    return names.join(".");
  }

  function signatureOf(subject) {
    var nodes = [subject].concat(controlledElements(subject));
    var parts = [];
    for (var i = 0; i < nodes.length; i += 1) {
      var node = nodes[i];
      parts.push(
        node.tagName +
          "|" +
          (node.getAttribute("aria-expanded") || "") +
          "|" +
          (node.hasAttribute("hidden") ? "hidden" : "-") +
          "|" +
          (node.open ? "open" : "-") +
          "|" +
          classState(node)
      );
    }
    if (subject.closest) {
      var details = subject.closest("details");
      if (details) parts.push("details|" + (details.open ? "open" : "-"));
      var dialog = subject.closest("dialog");
      if (dialog) parts.push("dialog|" + (dialog.open ? "open" : "-"));
    }
    return parts.join(";");
  }

  // The element whose toggles define the step: the button/summary/link that was clicked, else the node itself.
  function toggleSubject(node) {
    if (!node || node.nodeType !== 1 || !node.closest) return node;
    return (
      node.closest("button, summary, a, [role='button'], [aria-expanded]") || node
    );
  }

  function selectorOf(element) {
    var node = element;
    var parts = [];
    while (node && node.nodeType === 1) {
      if (node.id) {
        parts.unshift("#" + cssEscape(node.id));
        return parts.join(" > ");
      }
      parts.unshift(node.tagName.toLowerCase() + ":nth-of-type(" + nthOfType(node) + ")");
      node = node.parentElement;
    }
    return parts.join(" > ");
  }

  function updateState() {
    var steps = [];
    for (var i = 0; i < stateSteps.length; i += 1) steps.push("click:" + stateSteps[i].selector);
    var next = steps.join(";");
    if (next === state) return;
    state = next;
    scheduleLayout();
  }

  // A click toggled something: record it, or drop the step it undid (so closing the menu resets the recipe).
  function recordStep(selector, before, after) {
    for (var i = stateSteps.length - 1; i >= 0; i -= 1) {
      if (stateSteps[i].selector !== selector) continue;
      if (after === stateSteps[i].before) {
        stateSteps.splice(i, 1);
      } else {
        stateSteps[i].after = after;
      }
      updateState();
      return;
    }
    stateSteps.push({ selector: selector, before: before, after: after });
    updateState();
  }

  function beginClick(event) {
    var node = event.target;
    if (!node || node.nodeType !== 1) return;
    var subject = toggleSubject(node);
    var selector;
    var before;
    try {
      selector = selectorOf(subject);
      before = signatureOf(subject);
    } catch (_) {
      return;
    }
    var recorded = false;
    function check() {
      if (recorded) return;
      try {
        if (!subject.isConnected) return;
        var after = signatureOf(subject);
        if (after === before) return;
        recorded = true;
        recordStep(selector, before, after);
      } catch (_) {
        /* the page changed under us */
      }
    }
    // The page's own handler usually runs synchronously; the frame and timer cover async frameworks.
    Promise.resolve().then(check);
    window.setTimeout(check, 0);
    if (window.requestAnimationFrame) window.requestAnimationFrame(check);
  }

  function parseRecipe(recipe) {
    var steps = [];
    if (typeof recipe !== "string") return steps;
    var parts = recipe.split(";");
    for (var i = 0; i < parts.length; i += 1) {
      var step = parts[i].trim();
      if (!step) continue;
      if (step.slice(0, 6) !== "click:") return null;
      steps.push(step.slice(6));
    }
    return steps;
  }

  function querySelector(selector) {
    try {
      return document.querySelector(selector);
    } catch (_) {
      return null;
    }
  }

  // The caller has already navigated to the mark's route; a recipe is reset by navigation, so it names no route.
  function replay(recipe) {
    var steps = parseRecipe(recipe);
    if (steps === null) {
      send({ t: "pw:replayed", state: state, ok: false });
      return;
    }
    if (state === recipe) {
      send({ t: "pw:replayed", state: state, ok: true });
      return;
    }
    for (var i = 0; i < steps.length; i += 1) {
      var element = querySelector(steps[i]);
      if (!element) {
        send({ t: "pw:replayed", state: state, ok: false });
        return;
      }
      try {
        element.click();
      } catch (_) {
        send({ t: "pw:replayed", state: state, ok: false });
        return;
      }
    }
    window.setTimeout(function () {
      send({ t: "pw:replayed", state: state, ok: state === recipe });
    }, 0);
  }

  /* ------------------------------------------------------------------ patches */

  // The owner's frame shows a patch (a ghost or a variant) without touching the project's files. Everything is
  // recorded so `on=false` puts the DOM back exactly: removed/replaced elements wait behind a comment
  // placeholder, inserted nodes are references we remove again, and attribute/class/text changes carry their
  // old value. The CSS lives in one `<style data-pw-patch>`; `textContent` means it can never escape the tag.

  var FORBIDDEN_TAGS = { script: 1, iframe: 1, object: 1, embed: 1 };

  // `javascript:` with whitespace, controls and character references resolved (the server strips the same).
  function dangerousValue(value) {
    var text = String(value).replace(/&#(x?)([0-9a-f]+);?/gi, function (match, hex, digits) {
      var code = parseInt(digits, hex ? 16 : 10);
      return isFinite(code) ? String.fromCharCode(code) : match;
    });
    text = text.replace(/[\s\u0000-\u001f]/g, "").toLowerCase();
    return text.indexOf("javascript:") === 0;
  }

  // Removes forbidden elements and attributes from a parsed subtree, in place.
  function scrub(node) {
    if (!node) return;
    if (node.nodeType === 1) {
      var attributes = node.attributes;
      for (var i = attributes.length - 1; i >= 0; i -= 1) {
        var attribute = attributes[i];
        var name = attribute.name.toLowerCase();
        if (name.indexOf("on") === 0 || dangerousValue(attribute.value)) {
          node.removeAttribute(attribute.name);
        }
      }
    }
    var child = node.firstChild;
    while (child) {
      var next = child.nextSibling;
      if (child.nodeType === 1 && FORBIDDEN_TAGS[child.tagName.toLowerCase()] === 1) {
        node.removeChild(child);
      } else {
        scrub(child);
      }
      child = next;
    }
  }

  // Patch HTML as detached nodes: `template` parses fragments without running scripts, and never loads an
  // image or an iframe before we have scrubbed it (the fragment is inert until inserted).
  function fragment(html) {
    var template = document.createElement("template");
    if (template.content) {
      template.innerHTML = String(html);
      scrub(template.content);
      return Array.prototype.slice.call(template.content.childNodes);
    }
    var doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = String(html);
    scrub(doc.body);
    var nodes = [];
    while (doc.body.firstChild) nodes.push(doc.body.removeChild(doc.body.firstChild));
    return nodes;
  }

  function insertNodes(parent, nodes, before) {
    for (var i = 0; i < nodes.length; i += 1) {
      parent.insertBefore(nodes[i], before || null);
    }
  }

  function patchStyle() {
    return document.querySelector("style[" + PATCH_ATTR + "]");
  }

  function removePatchStyle() {
    var style = patchStyle();
    if (style && style.parentNode) style.parentNode.removeChild(style);
  }

  // The node an op's selector must find; null (reported) for a bad or unmatched selector.
  function queryOrFail(selector, index, failed) {
    if (typeof selector !== "string" || !selector) {
      if (selector !== undefined) failed.push({ index: index, reason: "bad selector" });
      return null;
    }
    var matches;
    try {
      matches = document.querySelectorAll(selector);
    } catch (_) {
      failed.push({ index: index, reason: "bad selector" });
      return null;
    }
    if (!matches.length) {
      failed.push({ index: index, reason: "no match" });
      return null;
    }
    return matches;
  }

  function replaceWithNodes(element, nodes) {
    var parent = element.parentNode;
    if (!parent) return false;
    var placeholder = document.createComment("pw-patch");
    parent.insertBefore(placeholder, element);
    insertNodes(parent, nodes, placeholder);
    parent.removeChild(element);
    patchUndo.push({
      kind: "removed",
      element: element,
      placeholder: placeholder,
      inserted: nodes
    });
    return true;
  }

  function applyOne(op, element, index, failed) {
    if (!element || element.nodeType !== 1) return;
    var html = op.html == null ? "" : op.html;
    var nodes;
    switch (op.op) {
      case "replace":
        nodes = fragment(html);
        if (!replaceWithNodes(element, nodes)) failed.push({ index: index, reason: "no parent" });
        return;
      case "remove":
        if (!replaceWithNodes(element, [])) failed.push({ index: index, reason: "no parent" });
        return;
      case "insert_before":
        nodes = fragment(html);
        insertNodes(element.parentNode, nodes, element);
        patchUndo.push({ kind: "inserted", nodes: nodes });
        return;
      case "insert_after":
        nodes = fragment(html);
        insertNodes(element.parentNode, nodes, element.nextSibling);
        patchUndo.push({ kind: "inserted", nodes: nodes });
        return;
      case "append":
        nodes = fragment(html);
        insertNodes(element, nodes, null);
        patchUndo.push({ kind: "inserted", nodes: nodes });
        return;
      case "prepend":
        nodes = fragment(html);
        insertNodes(element, nodes, element.firstChild);
        patchUndo.push({ kind: "inserted", nodes: nodes });
        return;
      case "set_attr":
        if (typeof op.name !== "string" || !op.name) {
          failed.push({ index: index, reason: "bad attribute" });
          return;
        }
        if (op.name.toLowerCase().indexOf("on") === 0 || dangerousValue(op.value == null ? "" : op.value)) {
          return;
        }
        patchUndo.push({ kind: "attr", element: element, name: op.name, value: element.getAttribute(op.name) });
        element.setAttribute(op.name, op.value == null ? "" : String(op.value));
        return;
      case "remove_attr":
        if (typeof op.name !== "string" || !op.name) {
          failed.push({ index: index, reason: "bad attribute" });
          return;
        }
        patchUndo.push({ kind: "attr", element: element, name: op.name, value: element.getAttribute(op.name) });
        element.removeAttribute(op.name);
        return;
      case "set_text":
        patchUndo.push({ kind: "text", element: element, text: element.textContent });
        element.textContent = op.value == null ? "" : String(op.value);
        return;
      case "add_class":
      case "remove_class":
        if (typeof op.name !== "string" || !op.name || /\s/.test(op.name)) {
          failed.push({ index: index, reason: "bad class" });
          return;
        }
        patchUndo.push({
          kind: "class",
          element: element,
          name: op.name,
          had: element.classList.contains(op.name)
        });
        if (op.op === "add_class") element.classList.add(op.name);
        else element.classList.remove(op.name);
        return;
      default:
        failed.push({ index: index, reason: "unknown op" });
    }
  }

  function applyOps(dom, failed) {
    if (!dom || typeof dom.length !== "number") return;
    for (var i = 0; i < dom.length; i += 1) {
      var op = dom[i];
      if (!op || typeof op !== "object") {
        failed.push({ index: i, reason: "bad op" });
        continue;
      }
      var matches = queryOrFail(op.selector, i, failed);
      if (!matches) continue;
      for (var j = 0; j < matches.length; j += 1) applyOne(op, matches[j], i, failed);
    }
  }

  function undoOne(record) {
    if (record.kind === "removed") {
      for (var i = 0; i < record.inserted.length; i += 1) {
        var inserted = record.inserted[i];
        if (inserted.parentNode) inserted.parentNode.removeChild(inserted);
      }
      var parent = record.placeholder.parentNode;
      if (parent) parent.replaceChild(record.element, record.placeholder);
    } else if (record.kind === "inserted") {
      for (var j = 0; j < record.nodes.length; j += 1) {
        var node = record.nodes[j];
        if (node.parentNode) node.parentNode.removeChild(node);
      }
    } else if (record.kind === "attr") {
      if (record.value === null) record.element.removeAttribute(record.name);
      else record.element.setAttribute(record.name, record.value);
    } else if (record.kind === "text") {
      record.element.textContent = record.text;
    } else if (record.kind === "class") {
      if (record.had) record.element.classList.add(record.name);
      else record.element.classList.remove(record.name);
    }
  }

  // Puts the DOM back exactly as it was before the patch (reverse order, so nesting unwinds).
  function revertPatch() {
    for (var i = patchUndo.length - 1; i >= 0; i -= 1) {
      try {
        undoOne(patchUndo[i]);
      } catch (_) {
        /* the page moved the node itself; keep reverting the rest */
      }
    }
    patchUndo = [];
    removePatchStyle();
    patchActive = false;
    scheduleLayout();
  }

  // Applies `patch` (`on` false reverts). Returns `{ok, failed}` synchronously, which the server's headless
  // validation reads; the host hears the same as `{t:"pw:patched"}`.
  function applyPatch(patch, on) {
    revertPatch();
    var failed = [];
    if (on === false) return { ok: true, failed: failed };
    if (typeof patch === "string") {
      try {
        patch = JSON.parse(patch);
      } catch (_) {
        patch = null;
      }
    }
    if (!patch || typeof patch !== "object") {
      failed.push({ index: 0, reason: "bad patch" });
      return { ok: false, failed: failed };
    }
    if (typeof patch.css === "string" && patch.css && document.head) {
      var style = document.createElement("style");
      style.setAttribute(PATCH_ATTR, "");
      style.textContent = patch.css;
      document.head.appendChild(style);
    }
    applyOps(patch.dom, failed);
    patchActive = true;
    scheduleLayout();
    return { ok: failed.length === 0, failed: failed };
  }

  function patchMessage(patch, on) {
    var result = applyPatch(patch, on);
    send({ t: "pw:patched", ok: result.ok, failed: result.failed });
    return result;
  }

  /* ------------------------------------------------------------------ hits and messages */

  function hit(id, x, y) {
    var element = document.elementFromPoint(x, y);
    if (!element) {
      send({ t: "pw:hit", id: id, key: null, selector: null, rect: null });
      return;
    }
    var rect = rectOf(element);
    send({
      t: "pw:hit",
      id: id,
      key: keyOf(element),
      selector: selectorOf(element),
      rect: [rect[0], rect[1], rect[2], rect[3]]
    });
  }

  function receive(data) {
    if (typeof data === "string") {
      try {
        data = JSON.parse(data);
      } catch (_) {
        return;
      }
    }
    if (!data || typeof data !== "object" || typeof data.t !== "string") return;
    if (data.t === "pw:hit?") {
      hit(data.id, Number(data.x) || 0, Number(data.y) || 0);
    } else if (data.t === "pw:watch") {
      watch(data.keys);
    } else if (data.t === "pw:replay") {
      replay(data.state);
    } else if (data.t === "pw:patch") {
      patchMessage(data.patch, data.on !== false);
    } else if (data.t === "pw:scrollTo") {
      window.scrollTo(Number(data.x) || 0, Number(data.y) || 0);
      scheduleScroll();
      scheduleLayout();
    } else if (data.t === "pw:navigate") {
      var target = typeof data.route === "string" ? data.route : "";
      // Same-origin paths only: the bridge never turns into an open redirect.
      if (target.charAt(0) !== "/" || target.charAt(1) === "/") return;
      saveScroll();
      if (target === route()) return;
      location.assign(target);
    }
  }

  // The Android WebView has no parent window: the host calls this directly (or window.postMessage).
  // The server's headless page also uses the resolvers (`resolveKey`, `selectorOf`, `stateOf`) to answer an
  // agent's mark bundle (design-harness §7.2 C2) without re-implementing the key format.
  window.PenwiseBridge = {
    receive: receive,
    resolveKey: resolveKey,
    selectorOf: selectorOf,
    stateOf: function () {
      return state;
    },
    // The server's headless pages apply a ghost/variant patch directly and read the same `{ok, failed}` a host
    // gets over `pw:patched`.
    applyPatch: function (patch, on) {
      return applyPatch(patch, on);
    },
    revertPatch: revertPatch,
    patched: function () {
      return patchActive;
    }
  };

  window.addEventListener(
    "message",
    function (event) {
      // Accept only the frame host (a page inside this one must not drive the bridge).
      if (event.source && event.source !== window.parent && event.source !== window) return;
      receive(event.data);
    },
    false
  );

  document.addEventListener("click", beginClick, true);

  function onViewportChange() {
    scheduleScroll();
    scheduleLayout();
  }

  window.addEventListener("scroll", onViewportChange, { passive: true });
  window.addEventListener("resize", onViewportChange, false);
  if (window.ResizeObserver && document.documentElement) {
    try {
      new window.ResizeObserver(onViewportChange).observe(document.documentElement);
    } catch (_) {
      /* no observer support */
    }
  }
  observeDom();

  function liveReload() {
    if (!window.WebSocket) return;
    var attempts = 0;
    function connect() {
      var protocol = location.protocol === "https:" ? "wss:" : "ws:";
      var socket;
      try {
        socket = new WebSocket(protocol + "//" + location.host + RELOAD_PATH);
      } catch (_) {
        return;
      }
      socket.onopen = function () {
        attempts = 0;
      };
      socket.onmessage = function (event) {
        var message = event.data;
        try {
          message = JSON.parse(message);
        } catch (_) {
          return;
        }
        if (message && message.t === "pw:reload") {
          saveScroll();
          location.reload();
        }
      };
      socket.onclose = function () {
        // A proxied project answers 404 here (its dev server does HMR); stop retrying after a few tries.
        attempts += 1;
        if (attempts <= 5) window.setTimeout(connect, 1000 * Math.min(attempts, 5));
      };
    }
    connect();
  }

  // Our own entry per route wins over the browser's guess (it also survives a live reload).
  try {
    if (window.history && "scrollRestoration" in window.history) {
      window.history.scrollRestoration = "manual";
    }
  } catch (_) {
    /* not supported */
  }
  hookHistory();
  lastRoute = route();
  window.addEventListener("pagehide", saveScroll, false);
  window.addEventListener(
    "pageshow",
    function (event) {
      if (event.persisted) restoreScroll();
    },
    false
  );

  if (document.readyState === "loading") {
    document.addEventListener(
      "DOMContentLoaded",
      function () {
        hello();
        scheduleLayout();
      },
      { once: true }
    );
  } else {
    hello();
    scheduleLayout();
  }
  window.addEventListener(
    "load",
    function () {
      restoreScroll();
      hello();
      scheduleScroll();
      scheduleLayout();
    },
    { once: true }
  );
  liveReload();
})();
