/* Site web de Winlux.
   1. Page racine : ouvre la langue du navigateur (ou celle choisie avant).
   2. Boutons de téléchargement : pointent vers les fichiers de la dernière
      version publiée (API publique des releases GitHub). Sans réseau ou si
      l'API refuse, ils gardent leur lien vers la page de la dernière version. */
(function () {
  "use strict";

  var LANGS = ["fr", "en", "de", "es"];
  var STRINGS = {
    fr: { version: "Version {v}", published: "publiée le {d}" },
    en: { version: "Version {v}", published: "released on {d}" },
    de: { version: "Version {v}", published: "veröffentlicht am {d}" },
    es: { version: "Versión {v}", published: "publicada el {d}" }
  };

  function store(key, value) {
    try {
      if (value === undefined) return window.localStorage.getItem(key);
      window.localStorage.setItem(key, value);
    } catch (e) { /* stockage indisponible : sans importance */ }
    return null;
  }

  // --- Langue -------------------------------------------------------------
  var page = document.documentElement.getAttribute("data-page");
  if (page === "root") {
    var chosen = store("winlux-lang");
    var wanted = LANGS.indexOf(chosen) >= 0 ? chosen : null;
    if (!wanted) {
      var prefs = navigator.languages || [navigator.language || ""];
      for (var i = 0; i < prefs.length && !wanted; i++) {
        var code = String(prefs[i]).slice(0, 2).toLowerCase();
        if (LANGS.indexOf(code) >= 0) wanted = code;
      }
    }
    if (wanted && !/[?&]choisir/.test(location.search)) {
      location.replace(wanted + "/index.html" + location.hash);
    }
  }
  document.addEventListener("click", function (ev) {
    var a = ev.target.closest && ev.target.closest("a[data-lang]");
    if (a) store("winlux-lang", a.getAttribute("data-lang"));
  });

  // --- Dernière version ---------------------------------------------------
  var boxes = document.querySelectorAll("[data-release]");
  if (!boxes.length || !window.fetch) return;
  var repo = document.body.getAttribute("data-repo") || "Univers4craft/winlux";
  var lang = document.body.getAttribute("data-lang") || "fr";
  var t = STRINGS[lang] || STRINGS.en;
  var CACHE = "winlux-release-v1";

  function size(bytes) {
    var units = lang === "en" ? ["B", "KB", "MB", "GB"] : ["o", "Ko", "Mo", "Go"];
    var n = bytes, u = 0;
    while (n >= 1000 && u < units.length - 1) { n /= 1000; u++; }
    var s = n.toLocaleString(lang, { maximumFractionDigits: 1 });
    return s + " " + units[u];
  }

  function pick(assets, re) {
    for (var i = 0; i < assets.length; i++) {
      if (re.test(assets[i].name)) return assets[i];
    }
    return null;
  }

  function apply(rel) {
    if (!rel || !rel.tag_name) return;
    var assets = rel.assets || [];
    var map = {
      iso: pick(assets, /^winlux-live-.*\.iso$/),
      sha256: pick(assets, /^winlux-live-.*\.iso\.sha256$/),
      "usb-exe": pick(assets, /^winlux-usb-creator.*\.exe$/),
      "usb-deb": pick(assets, /^winlux-usb-creator.*\.deb$/)
    };
    var date = "";
    try {
      date = new Date(rel.published_at).toLocaleDateString(lang, { day: "numeric", month: "long", year: "numeric" });
    } catch (e) { date = ""; }
    boxes.forEach(function (box) {
      var v = box.querySelector("[data-release-version]");
      if (v) v.textContent = t.version.replace("{v}", rel.tag_name);
      var d = box.querySelector("[data-release-date]");
      if (d && date) d.textContent = t.published.replace("{d}", date);
      Object.keys(map).forEach(function (key) {
        var asset = map[key];
        if (!asset) return;
        box.querySelectorAll('[data-asset="' + key + '"]').forEach(function (a) {
          a.href = asset.browser_download_url;
          a.setAttribute("data-file", asset.name);
        });
        var s = box.querySelector('[data-asset-size="' + key + '"]');
        if (s && asset.size) s.textContent = s.textContent.split(" · ")[0] + " · " + size(asset.size);
      });
    });
  }

  var cached = null;
  try { cached = JSON.parse(window.sessionStorage.getItem(CACHE) || "null"); } catch (e) { cached = null; }
  if (cached && Date.now() - cached.at < 3600 * 1000) {
    apply(cached.rel);
    return;
  }
  fetch("https://api.github.com/repos/" + repo + "/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
    .then(function (r) { return r.ok ? r.json() : null; })
    .then(function (rel) {
      if (!rel) return;
      var slim = {
        tag_name: rel.tag_name,
        published_at: rel.published_at,
        assets: (rel.assets || []).map(function (a) {
          return { name: a.name, size: a.size, browser_download_url: a.browser_download_url };
        })
      };
      try { window.sessionStorage.setItem(CACHE, JSON.stringify({ at: Date.now(), rel: slim })); } catch (e) { /* rien */ }
      apply(slim);
    })
    .catch(function () { /* hors ligne : les liens vers la dernière version restent */ });
})();
