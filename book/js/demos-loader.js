// Loads the book's interactive widgets on pages that have one (`<div class="etendue-demo"
// data-demo="…">`). The widgets are an ES module built from web/apps/book-demos into
// book/src/demos; a dynamic import resolves against this script's URL, so it works at any page
// depth. Without the module (a book built without it, or no wasm), each widget says so.
(function () {
  var nodes = document.querySelectorAll(".etendue-demo");
  if (nodes.length === 0) return;
  import("../demos/demos.js")
    .then(function (m) {
      m.mount(nodes);
    })
    .catch(function (e) {
      console.error("etendue widgets:", e);
      nodes.forEach(function (n) {
        n.setAttribute("data-state", "unavailable");
        n.textContent = "This interactive widget could not be loaded in this browser.";
      });
    });
})();
