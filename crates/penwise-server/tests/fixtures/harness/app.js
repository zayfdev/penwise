(function () {
  "use strict";

  var button = document.getElementById("menu-button");
  var menu = document.getElementById("menu");

  button.addEventListener("click", function () {
    var open = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", open ? "true" : "false");
    button.classList.toggle("open", open);
    menu.hidden = !open;
    menu.classList.toggle("open", open);
  });

  function rerender() {
    var list = document.getElementById("list");
    while (list.firstChild) list.removeChild(list.firstChild);
    ["a", "b"].forEach(function (name) {
      var row = document.createElement("li");
      row.className = "row";
      row.setAttribute("data-row", name);
      row.textContent = name.toUpperCase();
      list.appendChild(row);
    });
  }

  document.getElementById("rerender").addEventListener("click", rerender);

  window.pwHarness = { rerender: rerender };
})();
