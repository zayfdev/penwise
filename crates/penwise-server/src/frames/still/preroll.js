// Scrolls through the document in viewport steps so lazy images load and reveal-on-scroll content appears,
// then returns to the top. Arguments: viewport height, the document cap, the settle time per step and the
// total time budget (all CSS px / ms). Resolves with {steps, doc} once the page is back at scroll 0.
(function (viewport, cap, settle, budget) {
  return new Promise(function (resolve) {
    var started = Date.now();
    var root = document.documentElement;
    var body = document.body;
    var steps = 0;
    var y = 0;

    function height() {
      return Math.max(root ? root.scrollHeight : 0, body ? body.scrollHeight : 0);
    }

    function scrollTo(top) {
      // `instant` beats any `scroll-behavior: smooth` the page sets.
      window.scrollTo({ top: top, left: 0, behavior: "instant" });
    }

    function lastTop() {
      return Math.max(0, Math.min(height(), cap) - viewport);
    }

    function finish() {
      scrollTo(0);
      // Let reveal transitions and scroll-linked effects of the last step finish before anything is measured.
      setTimeout(function () {
        resolve({ steps: steps, doc: height() });
      }, 350);
    }

    function step() {
      y = Math.min(y, lastTop());
      scrollTo(y);
      steps += 1;
      setTimeout(function () {
        if (y >= lastTop() || Date.now() - started > budget) {
          finish();
          return;
        }
        y += viewport;
        step();
      }, settle);
    }

    step();
  });
})
