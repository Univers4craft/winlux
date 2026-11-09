#!/usr/bin/env python3
"""Store : logiciels publiés pour Linux par leur éditeur (#1056).

Catalogue integration/store-editeurs.json (4 langues, https, hôtes de
l'éditeur, aucune marque Microsoft), dépôt d'éditeur simulé en file:// :
confirmation montrant produit, version, éditeur, source et empreinte avant
tout téléchargement, refus = rien de téléchargé, empreinte et version
contrôlées, fichier dans un dossier neuf et vide, purge + retrait du dépôt
à la désinstallation (winlux-store-repos), relais root limité."""
import gzip
import hashlib
import json
import os
import re
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
CATS = {"bureautique", "internet", "multimedia", "graphisme", "developpement", "jeux", "education",
        "sciences", "utilitaires", "systeme", "accessibilite"}
BRAND = re.compile(r"microsoft|\bedge\b|msedge|\bbing\b|office\s*365|windows\s*11", re.I)


def check(cond, message):
    if not cond:
        print(f"ÉCHEC : {message}", file=sys.stderr)
        sys.exit(1)


# --- 1. Le catalogue livré ---------------------------------------------------
real = json.loads((project / "integration/store-editeurs.json").read_text())
ids = [e["id"] for e in real["apps"]]
check(len(ids) == len(set(ids)) and len(ids) >= 10, "identifiants uniques, au moins dix logiciels")
check({"claude", "chatgpt"} <= set(ids), "Claude et ChatGPT au catalogue")
check(ids == sorted(ids), "ordre alphabétique, aucun éditeur mis en avant")
for e in real["apps"]:
    name = e["id"]
    check(e["kind"] in ("index", "deb"), f"{name} : type connu")
    check(e["category"] in CATS, f"{name} : catégorie du Store")
    check(e["licence"] in ("libre", "proprietaire"), f"{name} : licence dite")
    check(all(e["desc"].get(l, "").strip() for l in ("fr", "en", "es", "de")), f"{name} : 4 langues")
    check(e["publisher"].strip() and e["site"].startswith("https://"), f"{name} : éditeur et site")
    url = e["base"] if e["kind"] == "index" else e["url"]
    host = re.match(r"https://([^/]+)/", url)
    check(host and host.group(1) in e["hosts"], f"{name} : adresse https chez l'éditeur ({url})")
    check(not BRAND.search(json.dumps(e, ensure_ascii=False)), f"{name} : aucune marque Microsoft")
    if e["kind"] == "deb" and e.get("sha256"):
        check(re.fullmatch(r"[0-9a-f]{64}", e["sha256"]) and e.get("version"), f"{name} : épinglé (version + SHA-256)")

with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    # --- 2. Dépôt d'éditeur simulé ------------------------------------------
    pkgdir = tmp / "build/claude-desktop"
    (pkgdir / "DEBIAN").mkdir(parents=True)
    (pkgdir / "DEBIAN/control").write_text("Package: claude-desktop\nVersion: 1.2.3\nArchitecture: amd64\n"
                                           "Maintainer: Test <t@example.org>\nDescription: test\n")
    (pkgdir / "usr/bin").mkdir(parents=True)
    (pkgdir / "usr/bin/claude-desktop").write_text("#!/bin/sh\n")
    repo = tmp / "repo"
    pool = repo / "pool/main/c"
    pool.mkdir(parents=True)
    deb = pool / "claude-desktop_1.2.3_amd64.deb"
    subprocess.run(["dpkg-deb", "--root-owner-group", "-b", str(pkgdir), str(deb)], check=True, capture_output=True)
    sha = hashlib.sha256(deb.read_bytes()).hexdigest()
    index = repo / "dists/stable/main/binary-amd64/Packages"
    index.parent.mkdir(parents=True)

    def write_index(digest, version="1.2.3", filename="pool/main/c/claude-desktop_1.2.3_amd64.deb"):
        index.write_text(f"Package: claude-desktop\nVersion: 1.0\nArchitecture: amd64\nFilename: {filename}\n"
                         f"SHA256: {'0' * 64}\nSize: 10\n\n"
                         f"Package: claude-desktop\nVersion: {version}\nArchitecture: amd64\nFilename: {filename}\n"
                         f"SHA256: {digest}\nSize: {deb.stat().st_size}\n\n"
                         f"Package: autre\nVersion: 9\nArchitecture: amd64\nFilename: pool/x.deb\nSHA256: {'1' * 64}\n")
    write_index(sha)
    defs = tmp / "editeurs.json"
    entry = {"id": "claude", "name": "Claude", "publisher": "Anthropic", "category": "bureautique",
             "licence": "proprietaire", "package": "claude-desktop", "kind": "index",
             "base": repo.as_uri() + "/", "index": "dists/stable/main/binary-amd64/Packages",
             "hosts": ["downloads.claude.ai"], "site": "https://claude.ai",
             "desc": {l: f"desc-{l}" for l in ("fr", "en", "es", "de")}}
    hidden = {**entry, "id": "cache", "kind": "deb", "url": "https://x.example/a.deb", "version": "", "sha256": ""}
    defs.write_text(json.dumps({"apps": [entry, hidden]}))
    confirm_log = tmp / "confirm.log"
    stub = tmp / "confirm.sh"
    stub.write_text(f"#!/bin/sh\nprintf '%s\\n' \"$@\" >> {confirm_log}\nexit ${{CONFIRM_ANSWER:-1}}\n")
    stub.chmod(0o755)
    run_dir = tmp / "run"
    run_dir.mkdir()
    os.environ.update({"WINLUX_STORE_EDITEURS": str(defs), "WINLUX_STORE_CONFIRM": str(stub),
                       "HOME": str(tmp / "home"), "XDG_CACHE_HOME": str(tmp / "cache"),
                       "XDG_DATA_HOME": str(tmp / "data"), "WINLUX_STORE_RUN": str(run_dir),
                       "WINLUX_CONTROL_ROOT": str(tmp / "root"), "WINLUX_LANG": "fr"})
    m = runpy.run_path(str(project / "scripts/winlux-store.py"), run_name="test_editeurs")
    ed = m["SOURCE_BY_NAME"]["editeur"]
    check([e["id"] for e in m["editeur_defs"]()] == ["claude"], "entrée « deb » sans empreinte masquée")

    calls = []
    def fake_apt(args, progress):
        path = Path(args[-1])
        if args[0] == "install":
            check(path.is_file() and [p.name for p in path.parent.iterdir()] == ["claude-desktop.deb"],
                  "paquet seul dans un dossier neuf")
            check(hashlib.sha256(path.read_bytes()).hexdigest() == sha, "paquet vérifié transmis à apt")
        calls.append(list(args))
        return 0
    m["SOURCE_BY_NAME"]["apt"].apt = fake_apt
    ed.installed = lambda package: ""
    admin_calls, run_answer = [], {"code": 0}
    m["admin"].__globals__["admin"] = lambda cmd: admin_calls.append(cmd) or cmd
    m["run"].__globals__["run"] = lambda cmd, *a, **k: (run_answer["code"], "")
    apps = ed.apps()
    check(len(apps) == 1 and apps[0].state == "a" and apps[0].available == "1.2.3", "version la plus récente de l'index")
    check(apps[0].summary == "desc-fr" and apps[0].publisher == "Anthropic", "description et éditeur")

    # Confirmation : 4 langues, tout ce qu'il faut savoir.
    info = m["editeur_info"](entry, True)
    check(info["sha256"] == sha and info["source"].endswith("claude-desktop_1.2.3_amd64.deb"), "source et empreinte")
    for lang, words in (("fr", ("Installer Claude ?", "Éditeur : Anthropic", "Version : 1.2.3", "Annuler")),
                        ("en", ("Install Claude?", "Publisher: Anthropic", "Version: 1.2.3", "Cancel")),
                        ("es", ("¿Instalar Claude?", "Editor: Anthropic", "Versión: 1.2.3", "Cancelar")),
                        ("de", ("Claude installieren?", "Herausgeber: Anthropic", "Version: 1.2.3", "Abbrechen"))):
        m["tr"].__globals__["LANG"] = lang
        title, text = m["confirmation_text"](entry, info)
        full = title + "\n" + text + "\n" + m["tr"]("ed_cancel")
        for w in words:
            check(w in full, f"confirmation {lang} : {w!r}")
        check(sha in text and info["source"] in text, f"confirmation {lang} : source et empreinte complètes")
    m["tr"].__globals__["LANG"] = "fr"

    # Refus : rien téléchargé, rien installé.
    os.environ["CONFIRM_ANSWER"] = "1"
    check(ed.install("claude", lambda *a: None) == 9 and not calls, "Annuler : rien n'est installé")
    cache = tmp / "cache/winlux/store"
    check(not list(cache.glob("editeur-*")), "Annuler : rien n'est téléchargé")
    check("--cancel" in confirm_log.read_text() and "Annuler" in confirm_log.read_text(), "bouton Annuler proposé")

    check(not admin_calls, "Annuler : aucune autorisation demandée")

    # Autorisation refusée (#1451) : demandée juste après la confirmation,
    # avant tout téléchargement ; rien n'est téléchargé, message clair.
    os.environ["CONFIRM_ANSWER"] = "0"
    run_answer["code"] = 1223 % 256
    check(ed.install("claude", lambda *a: None) == 1223 % 256 and not calls, "autorisation refusée : rien installé")
    check(admin_calls == [["winlux-store-repos", "check", "claude"]], "autorisation demandée avant le téléchargement")
    check(not list(cache.glob("editeur-*")), "autorisation refusée : rien téléchargé")
    check(m["MESSAGES"][1223 % 256] == "denied" and "rien n'a été téléchargé" in m["tr"]("denied"),
          "refus de l'autorisation expliqué")
    run_answer["code"] = 0
    admin_calls.clear()

    # Accord : téléchargé, vérifié, installé, dossier nettoyé.
    order = []
    real_fetch = ed.fetch_deb
    ed.fetch_deb = lambda *a: order.append("téléchargement") or real_fetch(*a)
    m["admin"].__globals__["admin"] = lambda cmd: order.append("autorisation") or admin_calls.append(cmd) or cmd
    steps = []
    check(ed.install("claude", lambda pct, msg, *a: steps.append((len(order), pct, msg))) == 0,
          "installation confirmée")
    check(order == ["autorisation", "téléchargement"], f"autorisation puis téléchargement ({order})")
    # #1455 : aucun pourcentage avant le vrai téléchargement (pas de 10 % figé
    # pendant l'attente de l'autorisation) ; ensuite un avancement mesuré.
    before = [s for s in steps if s[0] < 2]
    check(before and all(pct is None for _, pct, _ in before), f"pas de pourcentage avant le téléchargement ({before})")
    check(any("autorisation" in msg for _, _, msg in before), "attente de l'autorisation affichée")
    check(not any("Téléchargement" in msg for _, _, msg in before), "pas de « Téléchargement » avant l'autorisation")
    m["STORE_RUN"].mkdir(parents=True, exist_ok=True)
    m["Progress"]("install", "editeur:claude")(None, "x")
    check("percent=\n" in (m["STORE_RUN"] / "store-progress.txt").read_text(), "avancement inconnu : percent vide")
    ed.fetch_deb = real_fetch
    check(calls and calls[-1][:2] == ["install", "--"], "apt-get install -- <fichier>")
    check(not list(cache.glob("editeur-*")), "dossier de téléchargement retiré")

    # Empreinte ou version fausses : refus.
    calls.clear()
    write_index("f" * 64)
    check(ed.install("claude", lambda *a: None) == 5 and not calls, "empreinte fausse refusée")
    write_index(sha, version="1.2.4")
    check(ed.install("claude", lambda *a: None) == 10 and not calls, "version annoncée différente refusée")
    write_index(sha, filename="../../etc/passwd.deb")
    check(m["editeur_info"](entry, True) == {}, "chemin hors du dépôt refusé")
    write_index(sha)
    check(m["editeur_url"]({**entry, "base": "https://downloads.claude.ai/a/"}, "https://evil.example/x.deb") == "",
          "hôte étranger refusé")
    check(m["editeur_url"]({**entry, "base": "https://downloads.claude.ai/a/"}, "pool/x.deb")
          == "https://downloads.claude.ai/a/pool/x.deb", "hôte de l'éditeur accepté")
    flat = {**entry, "base": "https://download.sublimetext.com/", "index": "apt/stable/Packages",
            "hosts": ["download.sublimetext.com"]}
    check(m["editeur_url"](flat, "files/sublime-text_build-4215_amd64.deb")
          == "https://download.sublimetext.com/files/sublime-text_build-4215_amd64.deb",
          "dépôt plat : adresse relative à la racine (vérifiée par la sonde de la CI)")

    # Mise à jour visible, puis désinstallation : purge + dépôt retiré.
    ed.installed = lambda package: "1.0"
    check(ed.apps()[0].state == "u", "mise à jour visible")
    admin_calls.clear()
    m["admin"].__globals__["admin"] = lambda cmd: admin_calls.append(cmd) or cmd
    check(ed.remove("claude", lambda *a: None) == 0, "désinstallation")
    check(calls[-1] == ["purge", "--autoremove", "--", "claude-desktop"], "purge du paquet")
    check(["winlux-store-repos", "clean", "claude"] in admin_calls, "dépôt de l'éditeur retiré")

    # --- 3. winlux-store-repos sur une racine simulée -----------------------
    root = tmp / "root"
    lists = root / "etc/apt/sources.list.d"
    lists.mkdir(parents=True)
    keys = root / "usr/share/keyrings"
    keys.mkdir(parents=True)
    (keys / "claude.asc").write_text("k")
    (keys / "commun.gpg").write_text("k")
    (lists / "claude-desktop.list").write_text(
        "# dépôt\ndeb [arch=amd64,arm64 signed-by=/usr/share/keyrings/claude.asc] https://downloads.claude.ai/claude-desktop/apt/stable stable main\n")
    (lists / "claude.sources").write_text("Types: deb\nURIs: https://downloads.claude.ai/x\nSuites: stable\nSigned-By: /usr/share/keyrings/commun.gpg\n")
    (lists / "melange.list").write_text("deb https://downloads.claude.ai/x stable main\ndeb [signed-by=/usr/share/keyrings/commun.gpg] https://archive.ubuntu.com/ubuntu noble main\n")
    bindir = tmp / "bin"
    bindir.mkdir()
    (bindir / "dpkg-query").write_text("#!/bin/sh\nprintf '%s' \"${FAKE_STATUS:-un }\"\n")
    (bindir / "dpkg-query").chmod(0o755)
    env = {**os.environ, "PATH": f"{bindir}:{os.environ['PATH']}", "WINLUX_CONTROL_ROOT": str(root)}
    tool = [sys.executable, "-I", str(project / "scripts/winlux-store-repos.py")]
    r = subprocess.run(tool + ["clean", "claude"], env={**env, "FAKE_STATUS": "ii "}, capture_output=True, text=True)
    check(r.returncode == 3 and (lists / "claude-desktop.list").exists(), "dépôt gardé tant que le paquet est installé")
    r = subprocess.run(tool + ["clean", "claude"], env=env, capture_output=True, text=True)
    check(r.returncode == 0, f"nettoyage : {r.stderr}")
    check(not (lists / "claude-desktop.list").exists() and not (lists / "claude.sources").exists(), "listes de l'éditeur retirées")
    check((lists / "melange.list").exists(), "liste mêlée à un autre dépôt gardée")
    check(not (keys / "claude.asc").exists() and (keys / "commun.gpg").exists(), "clé retirée seulement si plus utilisée")
    r = subprocess.run(tool + ["clean", "../x"], env=env, capture_output=True, text=True)
    check(r.returncode == 64, "identifiant invalide refusé")

# --- 4. Relais root, confirmation, image ------------------------------------
wr = runpy.run_path(str(project / "scripts/winlux-root.py"), run_name="test_editeurs")
check(wr["exec_level"](["winlux-store-repos", "clean", "claude"]) == "admin", "relais : nettoyage en administrateur")
check(wr["exec_level"](["winlux-store-repos", "clean", "../etc"]) is None, "relais : chemin refusé")
check(wr["exec_level"](["winlux-store-repos", "check", "claude"]) == "admin", "relais : autorisation avant téléchargement")
check(wr["exec_level"](["winlux-store-repos", "rm", "claude"]) is None, "relais : autre commande refusée")
confirm = (project / "scripts/winlux-store-confirm.py").read_text()
check("set_default_response(Gtk.ResponseType.CANCEL)" in confirm and "cancel.grab_focus()" in confirm,
      "Annuler par défaut")
check('cancel.get_style_context().add_class("suggested-action")' in confirm and "set_focus_visible(True)" in confirm,
      "bouton actif visible (#1448)")

# --- 5. Store (shell) : croix de recherche, résultat gardé, barre latérale ---
rs = (project / "testexe/src/shell/store.rs").read_text(encoding="latin-1")
draw = rs[rs.index("hit(sr, Action::Search);"):]
check(draw.index("hit(sr, Action::Search);") < draw.index("hit(cr, Action::ClearSearch);"),
      "croix enregistrée après le champ : elle l'emporte au clic (#1447)")
check("Action::DismissResult => RESULT.clear()" in rs and "RESULT_ERROR || GetTickCount().wrapping_sub(RESULT_AT) < RESULT_MS" in rs
      and "RESULT_MS: u32 = 12_000" in rs, "résultat gardé dans le Store, échec jusqu'à la croix (#1449)")
labels = re.search(r"fn rail_refresh_label\(\).*?\n}", rs, re.S).group(0)
for label in re.findall(r'=> b"([^"]*)"', labels):
    check(len(label.encode("latin-1").decode("unicode_escape")) <= 11, f"barre latérale : « {label} » tient en 88 px (#1450)")
check('b"Neu laden"' in labels, "barre latérale allemande : « Neu laden »")
check("OP_PERCENT = if v.is_empty() { -1 }" in rs and "if pct < 0" in rs and "if OP_PERCENT >= 0" in rs,
      "avancement inconnu : ni chiffre ni barre remplie (#1455)")
r = subprocess.run([sys.executable, "-I", str(project / "scripts/winlux-store-confirm.py"), "--title", "t", "--text", "x",
                    "--ok", "o", "--cancel", "c"], env={k: v for k, v in os.environ.items()
                                                       if k not in ("DISPLAY", "WAYLAND_DISPLAY")},
                   capture_output=True, text=True, timeout=60)
check(r.returncode != 0, "sans affichage : refus")
iso = (project / "scripts/build-live-iso.sh").read_text()
for needle in ("store-editeurs.json", "winlux-store-confirm", "winlux-store-repos"):
    check(needle in iso, f"image : {needle}")
print("OK: logiciels des éditeurs (Claude, ChatGPT…) : confirmation, empreinte, version, purge et dépôt retiré, 4 langues (#1056)")
