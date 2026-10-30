// Every visible element of the page as {key, selector, rect}, topmost first, rects in document coordinates.
// Argument: the most elements to report. The order approximates paint order, so the first entry containing a
// point is what `document.elementFromPoint` would return there: stacking contexts (positioned, fixed/sticky,
// translucent, transformed or filtered elements) paint above ordinary content, by z-index and then document
// order; anything inside one paints with it.
(function (limit) {
  var bridge = window.PenwiseBridge;
  if (!bridge || !bridge.keyOf || !bridge.selectorOf) return [];
  var sx = window.pageXOffset || 0;
  var sy = window.pageYOffset || 0;
  var nodes = [document.documentElement].concat(Array.prototype.slice.call(document.querySelectorAll("*")));
  var order = new Map();
  var styles = new Map();
  nodes.forEach(function (node, index) {
    order.set(node, index);
  });

  function style(node) {
    var known = styles.get(node);
    if (!known) {
      known = window.getComputedStyle(node);
      styles.set(node, known);
    }
    return known;
  }

  // Where `node` paints inside its parent's context: [bucket, z-index, document order].
  function slot(node) {
    var cs = style(node);
    var positioned = cs.position !== "static";
    var forms =
      positioned ||
      parseFloat(cs.opacity) < 1 ||
      cs.transform !== "none" ||
      cs.filter !== "none" ||
      cs.isolation === "isolate";
    var z = parseInt(cs.zIndex, 10);
    if (!forms) return { forms: false, key: [1, 0, order.get(node)] };
    if (isNaN(z)) z = 0;
    return { forms: true, key: [z < 0 ? 0 : z > 0 ? 3 : 2, z, order.get(node)] };
  }

  var chains = new Map();
  // The slots of every context-forming ancestor, outermost first, then the node's own slot.
  function chain(node) {
    var known = chains.get(node);
    if (known) return known;
    var parent = node.parentElement;
    var above = parent ? contexts(parent) : [];
    var own = slot(node);
    known = above.concat([own.key]);
    chains.set(node, known);
    return known;
  }

  var contextChains = new Map();
  function contexts(node) {
    var known = contextChains.get(node);
    if (known) return known;
    var parent = node.parentElement;
    var above = parent ? contexts(parent) : [];
    var own = slot(node);
    known = own.forms ? above.concat([own.key]) : above;
    contextChains.set(node, known);
    return known;
  }

  function compare(a, b) {
    var length = Math.min(a.length, b.length);
    for (var i = 0; i < length; i += 1) {
      for (var j = 0; j < 3; j += 1) {
        if (a[i][j] !== b[i][j]) return a[i][j] - b[i][j];
      }
    }
    return a.length - b.length;
  }

  var found = [];
  for (var i = 0; i < nodes.length && found.length < limit; i += 1) {
    var node = nodes[i];
    var rect = node.getBoundingClientRect();
    if (!(rect.width > 0 && rect.height > 0)) continue;
    var cs = style(node);
    if (cs.visibility === "hidden" || cs.pointerEvents === "none") continue;
    found.push({ node: node, rect: rect, chain: chain(node) });
  }
  found.sort(function (a, b) {
    return compare(b.chain, a.chain);
  });
  return found.map(function (entry) {
    return {
      key: bridge.keyOf(entry.node),
      selector: bridge.selectorOf(entry.node),
      rect: [entry.rect.left + sx, entry.rect.top + sy, entry.rect.width, entry.rect.height]
    };
  });
})
