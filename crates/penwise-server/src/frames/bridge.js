/*
 * Penwise bridge — injected into every HTML page served by a project port.
 *
 * Contract (docs/design-harness-spec.md §6.2, C3):
 *   out: {t:"pw:hello", url, docW, docH, vw, vh}      on load
 *        {t:"pw:scroll", x, y, docW, docH}            rAF-throttled
 *        {t:"pw:hit", id, key, selector, rect}        in answer to a hit request
 *        {t:"pw:route", route}                        when the location changes (pushState/popstate/hash)
 *   in:  {t:"pw:hit?", id, x, y}                      x/y are viewport coordinates
 *        {t:"pw:scrollTo", x, y}
 *        {t:"pw:navigate", route}                     same-origin path; a full navigation
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
  var parent = window.parent && window.parent !== window ? window.parent : null;
  var host = window[HOST_JS] || null;
  var scrollFrame = 0;
  var lastScroll = "";
  var lastHello = "";
  var lastRoute = "";

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

  // `{t:"pw:route"}` whenever the location changes under the page (SPA routing, back/forward, hash).
  function reportRoute() {
    var now = route();
    if (now === lastRoute) return;
    lastRoute = now;
    send({ t: "pw:route", route: now });
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
    scrollFrame = 0;
    var pos = scrollPos();
    var size = docSize();
    var key = pos.x + "," + pos.y + "," + size.w + "," + size.h;
    if (key === lastScroll) return;
    lastScroll = key;
    send({ t: "pw:scroll", x: pos.x, y: pos.y, docW: size.w, docH: size.h });
  }

  function scheduleScroll() {
    if (scrollFrame) return;
    scrollFrame = window.requestAnimationFrame
      ? window.requestAnimationFrame(scrollNow)
      : window.setTimeout(scrollNow, 16);
  }

  function nthOfType(element) {
    var index = 1;
    for (var sibling = element.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
      if (sibling.tagName === element.tagName) index += 1;
    }
    return index;
  }

  function domPath(element) {
    var parts = [];
    for (var node = element; node && node.nodeType === 1; node = node.parentElement) {
      parts.unshift(
        node.tagName.toLowerCase() + ":nth-of-type(" + nthOfType(node) + ")"
      );
    }
    return parts.join(" > ");
  }

  function classSuffix(element) {
    var list = element.classList;
    if (!list || !list.length) return "";
    var names = [];
    for (var i = 0; i < list.length; i += 1) names.push(list[i]);
    return "." + names.join(".");
  }

  // The element fingerprint of the contract: tag + id + classes + nth-of-type path.
  function keyOf(element) {
    return (
      element.tagName.toLowerCase() +
      (element.id ? "#" + element.id : "") +
      classSuffix(element) +
      "|" +
      domPath(element)
    );
  }

  // A selector the agent can paste into DevTools: `#id` when unique, else the nth-of-type path.
  function selectorOf(element) {
    if (element.id) {
      return "#" + (window.CSS && window.CSS.escape ? window.CSS.escape(element.id) : element.id);
    }
    return domPath(element);
  }

  function rectOf(element) {
    var r = element.getBoundingClientRect();
    return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)];
  }

  function hit(id, x, y) {
    var element = document.elementFromPoint(x, y);
    if (!element) {
      send({ t: "pw:hit", id: id, key: null, selector: null, rect: null });
      return;
    }
    send({
      t: "pw:hit",
      id: id,
      key: keyOf(element),
      selector: selectorOf(element),
      rect: rectOf(element)
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
    } else if (data.t === "pw:scrollTo") {
      window.scrollTo(Number(data.x) || 0, Number(data.y) || 0);
      scheduleScroll();
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
  window.PenwiseBridge = { receive: receive };

  window.addEventListener(
    "message",
    function (event) {
      // Accept only the frame host (a page inside this one must not drive the bridge).
      if (event.source && event.source !== window.parent && event.source !== window) return;
      receive(event.data);
    },
    false
  );

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

  window.addEventListener("scroll", scheduleScroll, { passive: true });
  window.addEventListener("resize", scheduleScroll, false);
  if (window.ResizeObserver && document.documentElement) {
    try {
      new window.ResizeObserver(scheduleScroll).observe(document.documentElement);
    } catch (_) {
      /* no observer support */
    }
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
    document.addEventListener("DOMContentLoaded", hello, { once: true });
  } else {
    hello();
  }
  window.addEventListener(
    "load",
    function () {
      restoreScroll();
      hello();
      scheduleScroll();
    },
    { once: true }
  );
  liveReload();
})();
