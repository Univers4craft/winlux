#!/usr/bin/env node
// Rend chaque page du site de Winlux dans Chromium (Playwright) à plusieurs
// tailles d'écran et vérifie la mise en page. Appelé par scripts/test-site.py :
//   node scripts/test-site-render.js DOSSIER_DU_SITE [DOSSIER_DES_CAPTURES]
// Le réseau est coupé : la page doit rester correcte sans l'API GitHub.
"use strict";

const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

const site = path.resolve(process.argv[2]);
const shots = process.argv[3] ? path.resolve(process.argv[3]) : "";
const config = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "site", "site.json"), "utf8"));

const VIEWPORTS = [
  { name: "mobile", width: 360, height: 740 },
  { name: "tablette", width: 820, height: 1180 },
  { name: "portable", width: 1366, height: 768 },
  { name: "grand-ecran", width: 2560, height: 1440 },
];

const problems = [];
const fail = (msg) => { problems.push(msg); console.log("ÉCHEC : " + msg); };

(async () => {
  const browser = await chromium.launch();
  if (shots) fs.mkdirSync(shots, { recursive: true });
  let count = 0;
  for (const vp of VIEWPORTS) {
    for (const scheme of ["dark", "light"]) {
      const context = await browser.newContext({
        viewport: { width: vp.width, height: vp.height },
        colorScheme: scheme,
        locale: "de-DE",
      });
      await context.route(/^https?:\/\//, (route) => route.abort());
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      // En sombre, toutes les pages ; en clair, l'accueil et une page du guide.
      const list = [];
      for (const lang of config.languages) {
        for (const slug of config.pages) {
          if (scheme === "light" && !["index", "guide-install"].includes(slug)) continue;
          list.push(`${lang}/${slug}.html`);
        }
      }
      for (const rel of list) {
        const file = path.join(site, rel);
        await page.goto("file://" + file, { waitUntil: "load" });
        // Charge toutes les images (chargement différé) avant de mesurer.
        await page.evaluate(async () => {
          const imgs = Array.from(document.images);
          imgs.forEach((i) => { i.loading = "eager"; });
          await Promise.all(imgs.map((i) => (i.complete ? null : new Promise((r) => { i.onload = i.onerror = r; }))));
        });
        const m = await page.evaluate(() => {
          const doc = document.documentElement;
          const vis = (el) => {
            if (!el) return false;
            const r = el.getBoundingClientRect();
            const s = getComputedStyle(el);
            return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
          };
          const wide = [];
          document.querySelectorAll("main *, header *, footer *").forEach((el) => {
            const r = el.getBoundingClientRect();
            if (r.right > window.innerWidth + 1 && !el.closest(".table-wrap, pre, .mainnav ul")) {
              wide.push(el.tagName.toLowerCase() + (el.className ? "." + String(el.className).split(" ")[0] : ""));
            }
          });
          const broken = Array.from(document.images).filter((i) => !i.naturalWidth).map((i) => i.getAttribute("src"));
          const p = document.querySelector("main p");
          const prose = document.querySelector(".prose, .home");
          const h1 = document.querySelector("h1");
          const fontSize = p ? parseFloat(getComputedStyle(p).fontSize) : 0;
          const lineWidth = p ? p.getBoundingClientRect().width / (fontSize || 16) : 0;
          return {
            scrollWidth: doc.scrollWidth,
            innerWidth: window.innerWidth,
            wide: wide.slice(0, 5),
            broken,
            navVisible: vis(document.querySelector(".mainnav")),
            langsVisible: vis(document.querySelector(".langs")),
            h1Visible: vis(h1),
            fontSize,
            lineEm: lineWidth,
            proseWidth: prose ? prose.getBoundingClientRect().width : 0,
            title: document.title,
          };
        });
        const where = `${rel} [${vp.name}, ${scheme}]`;
        if (m.scrollWidth > m.innerWidth + 1) fail(`${where} : défilement horizontal (${m.scrollWidth} px pour ${m.innerWidth}) ${m.wide.join(" ")}`);
        else if (m.wide.length) fail(`${where} : éléments qui dépassent : ${m.wide.join(" ")}`);
        if (m.broken.length) fail(`${where} : images non chargées : ${m.broken.join(", ")}`);
        if (!m.navVisible) fail(`${where} : navigation invisible`);
        if (!m.langsVisible) fail(`${where} : choix de la langue invisible`);
        if (!m.h1Visible) fail(`${where} : titre principal invisible`);
        if (m.fontSize < 15) fail(`${where} : texte trop petit (${m.fontSize} px)`);
        // Une ligne de texte lisible : pas plus de ~90 caractères (≈ 48 em) sur grand écran.
        if (m.lineEm > 52 && !rel.endsWith("/index.html")) fail(`${where} : lignes de texte trop longues (${m.lineEm.toFixed(0)} em)`);
        if (vp.name === "grand-ecran" && m.proseWidth > 1500) fail(`${where} : contenu étiré sur toute la largeur (${m.proseWidth} px)`);
        if (!m.title.includes("Winlux")) fail(`${where} : titre de page sans « Winlux »`);
        if (shots && (rel.endsWith("/index.html") || rel.endsWith("/guide-install.html")) && (scheme === "dark" || rel.startsWith("fr/"))) {
          await page.screenshot({ path: path.join(shots, `${rel.replace("/", "-").replace(".html", "")}-${vp.name}-${scheme}.png`), fullPage: true });
        }
        count++;
      }
      if (errors.length) fail(`erreurs JavaScript (${vp.name}, ${scheme}) : ${errors.join(" | ")}`);
      // La page racine choisit la langue du navigateur (ici l'allemand).
      await page.goto("file://" + path.join(site, "index.html"));
      await page.waitForURL(/\/de\/index\.html$/, { timeout: 5000 }).catch(() => fail(`la page d'accueil n'ouvre pas la langue du navigateur (${page.url()})`));
      await context.close();
    }
  }
  await browser.close();
  console.log(`${count} rendus vérifiés (${VIEWPORTS.map((v) => v.width + "×" + v.height).join(", ")}, thèmes sombre et clair).`);
  process.exit(problems.length ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });
