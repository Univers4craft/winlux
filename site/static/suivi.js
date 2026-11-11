/* Galerie « Suivi des travaux » (#1779).
   Lit suivi/data.json, écrit par scripts/suivi-travaux.py à chaque publication
   du site après une exécution de la CI, et affiche une section par exécution,
   de la plus récente à la plus ancienne : commit, PR, date, puis les vignettes
   avec leur légende et le test qui les a prises. Sans data.json (site construit
   hors publication), le message vide reste. */
(function () {
  "use strict";
  var box = document.querySelector("[data-suivi]");
  if (!box || !window.fetch) return;
  var lang = document.body.getAttribute("data-lang") || "fr";
  var repo = document.body.getAttribute("data-repo") || "Univers4craft/winlux";
  var base = box.getAttribute("data-suivi");

  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }
  function link(href, text) {
    var a = el("a", "", text);
    a.href = href;
    a.rel = "noopener";
    return a;
  }
  function empty() {
    box.textContent = "";
    box.appendChild(el("p", "muted", box.getAttribute("data-empty")));
  }
  function when(iso) {
    try {
      return new Date(iso).toLocaleString(lang, { day: "numeric", month: "long", year: "numeric", hour: "2-digit", minute: "2-digit" });
    } catch (e) { return iso || ""; }
  }

  fetch(base + "data.json", { cache: "no-cache" })
    .then(function (r) { return r.ok ? r.json() : null; })
    .then(function (data) {
      var runs = data && data.executions;
      if (!runs || !runs.length) { empty(); return; }
      box.textContent = "";
      // Date des captures les plus récentes, et avertissement quand la CI n'a
      // pu que reprendre la galerie précédente (#1871).
      if (data.genere) box.appendChild(el("p", "suivi-meta muted", box.getAttribute("data-updated") + " " + when(data.genere)));
      if (data.reprise) box.appendChild(el("p", "suivi-failed", box.getAttribute("data-stale")));
      runs.forEach(function (run) {
        var sec = el("section", "suivi-run");
        var head = el("h2");
        head.appendChild(document.createTextNode((run.titre || run.branche || "") + " "));
        sec.appendChild(head);
        var meta = el("p", "suivi-meta muted");
        var commit = String(run.commit || "").slice(0, 7);
        if (commit) meta.appendChild(link("https://github.com/" + repo + "/commit/" + run.commit, commit));
        if (run.pr) {
          meta.appendChild(document.createTextNode(" · "));
          meta.appendChild(link("https://github.com/" + repo + "/pull/" + run.pr, box.getAttribute("data-pr") + " #" + run.pr));
        } else if (run.branche) {
          meta.appendChild(document.createTextNode(" · " + run.branche));
        }
        meta.appendChild(document.createTextNode(" · " + when(run.date) + " · "));
        meta.appendChild(link(run.url, box.getAttribute("data-run") + " " + (run.numero || run.run)));
        if (run.conclusion && run.conclusion !== "success") {
          meta.appendChild(document.createTextNode(" · "));
          meta.appendChild(el("span", "suivi-failed", box.getAttribute("data-failed")));
        }
        sec.appendChild(meta);
        var grid = el("div", "suivi-grid");
        run.images.forEach(function (img) {
          var fig = el("figure", "suivi-shot");
          var a = link(base + img.image, "");
          a.target = "_blank";
          var pic = el("img");
          pic.src = base + img.vignette;
          pic.loading = "lazy";
          pic.decoding = "async";
          var caption = (img.legende && (img.legende[lang] || img.legende.fr)) || img.nom;
          pic.alt = caption;
          pic.width = 480;
          pic.height = Math.round(480 * img.hauteur / img.largeur) || 270;
          a.appendChild(pic);
          fig.appendChild(a);
          var cap = el("figcaption");
          cap.appendChild(el("strong", "", caption));
          cap.appendChild(el("span", "muted", box.getAttribute("data-test") + " : " + img.test));
          fig.appendChild(cap);
          grid.appendChild(fig);
        });
        sec.appendChild(grid);
        box.appendChild(sec);
      });
    })
    .catch(empty);
})();
