(function () {
  "use strict";
  var grid = document.getElementById("grid");
  var empty = document.getElementById("empty");
  var search = document.getElementById("search");
  var sort = document.getElementById("sort");
  var kindButtons = Array.prototype.slice.call(document.querySelectorAll(".kinds button"));
  var cards = Array.prototype.slice.call(grid.children);
  var kind = "";
  var sorters = {
    newest: function (a, b) {
      return b.dataset.created.localeCompare(a.dataset.created) ||
             a.dataset.name.localeCompare(b.dataset.name);
    },
    oldest: function (a, b) { return -sorters.newest(a, b); },
    name: function (a, b) { return a.dataset.name.localeCompare(b.dataset.name); },
    size: function (a, b) { return (+a.dataset.size) - (+b.dataset.size); }
  };

  function apply() {
    var q = search.value.trim().toLowerCase();
    var shown = 0;
    cards.sort(sorters[sort.value] || sorters.newest);
    cards.forEach(function (card) {
      var ok = (!kind || card.dataset.kind === kind) &&
               (!q || card.dataset.text.indexOf(q) !== -1);
      card.hidden = !ok;
      if (ok) shown++;
      grid.appendChild(card);
    });
    empty.hidden = shown > 0;
  }

  apply();
  search.addEventListener("input", apply);
  sort.addEventListener("change", apply);
  kindButtons.forEach(function (btn) {
    btn.addEventListener("click", function () {
      kind = btn.dataset.kind;
      kindButtons.forEach(function (b) { b.classList.toggle("active", b === btn); });
      apply();
    });
  });
})();
