#!/usr/bin/python3
"""Boutiques de logiciels (docs/BOUTIQUES.md) et lanceur du Store.

Sans root ni réseau : racine factice (WINLUX_CONTROL_ROOT), apt-get et flatpak
remplacés par des faux qui notent leurs arguments, clé OpenPGP générée par gpg
pour simuler celle de l'éditeur. Vérifie le choix (aucune boutique d'office,
ordre alphabétique, 4 langues), l'ajout avec reprise sans réseau, le refus
d'une clé différente, le retrait, le lanceur épinglé une fois et le Store
qui ne montre que les boutiques choisies.
"""
import ast
import configparser
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tempfile
import unicodedata

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
TOOL = SCRIPTS / "winlux-store-catalogs.py"
DEFS = ROOT / "integration/store-catalogs.json"
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        raise SystemExit(1)
    checks += 1
    print(f"  OK     {label}")


def fake(directory, name, body):
    path = directory / name
    path.write_text("#!/bin/sh\n" + body + "\n")
    path.chmod(0o755)


def desktop(path):
    entry = configparser.RawConfigParser(strict=False, interpolation=None)
    entry.optionxform = str
    entry.read(path, encoding="utf-8")
    return entry["Desktop Entry"]


# ── Liste des boutiques proposées ───────────────────────────────────────────
defs = json.loads(DEFS.read_text(encoding="utf-8"))["catalogs"]
ids = [c["id"] for c in defs]
ok(sorted(ids) == ["flathub", "scoop-extras", "scoop-games"], "boutiques proposées : Flathub, Scoop Extras, Scoop Games")
ok(all(set(c["summary"]) == {"fr", "en", "es", "de"} and all(c["summary"].values()) for c in defs),
   "chaque boutique est décrite dans les quatre langues")
ok(all(c.get("repo", c.get("url", "")).startswith("https://") and c["site"].startswith("https://") for c in defs),
   "tout se télécharge en HTTPS chez l'éditeur")
banned = re.compile(r"winget|microsoft|chocolatey|snapcraft|appimage", re.I)
ok(not banned.search(DEFS.read_text(encoding="utf-8")), "aucune boutique écartée (Microsoft, winget, Chocolatey, Snap, AppImageHub)")
flathub = next(c for c in defs if c["id"] == "flathub")
ok(re.fullmatch(r"[0-9A-F]{40}", flathub["fingerprint"].replace(" ", "")),
   "empreinte de la clé de Flathub épinglée (40 chiffres hexadécimaux)")
ok(not list((ROOT / "integration/store-sources.d").glob("*.json")) if (ROOT / "integration/store-sources.d").is_dir() else True,
   "aucun catalogue Windows actif d'office dans l'image")
doc = (ROOT / "docs/BOUTIQUES.md").read_text(encoding="utf-8")
for word in ("Flathub", "Scoop Extras", "Scoop Games", "Snap Store", "AppImageHub", "Chocolatey", "winget",
             "Microsoft Store", "Ninite", "PortableApps", "F-Droid", "Steam"):
    ok(word in doc, f"docs/BOUTIQUES.md explique le sort de {word}")

# ── Empreinte OpenPGP : même calcul que gpg ─────────────────────────────────
lib = runpy.run_path(str(TOOL), run_name="winlux_store_catalogs")
with tempfile.TemporaryDirectory() as work:
    work = Path(work)
    gnupg = work / "gnupg"
    gnupg.mkdir(mode=0o700)
    env = {**os.environ, "GNUPGHOME": str(gnupg)}
    have_gpg = shutil.which("gpg") is not None
    if have_gpg:
        subprocess.run(["gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--quick-gen-key",
                        "Editeur de test <test@example.org>", "rsa2048", "sign", "never"], env=env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        key = subprocess.run(["gpg", "--batch", "--export"], env=env, check=True, capture_output=True).stdout
        colons = subprocess.run(["gpg", "--batch", "--with-colons", "--fingerprint"], env=env, check=True,
                                capture_output=True, text=True).stdout
        expected = next(line.split(":")[9] for line in colons.splitlines() if line.startswith("fpr:"))
        ok(lib["openpgp_fingerprint"](key) == expected, "empreinte OpenPGP v4 identique à celle de gpg")
    else:
        # Sans gpg : clé v4 minimale construite à la main.
        body = bytes([4]) + b"\0\0\0\0" + bytes([1]) + b"\x00\x08\x80" + b"\x00\x08\x03"
        key = b"\x99" + len(body).to_bytes(2, "big") + body
        import hashlib
        expected = hashlib.sha1(key).hexdigest().upper()
        ok(lib["openpgp_fingerprint"](key) == expected, "empreinte OpenPGP v4 (clé construite, gpg absent)")
    ok(lib["openpgp_fingerprint"](b"\x00\x01\x02") is None, "données qui ne sont pas une clé : refusées")
    import base64
    repo = work / "flathub.flatpakrepo"
    repo.write_text("[Flatpak Repo]\nTitle=Test\nUrl=https://example.org/repo/\n"
                    f"GPGKey={base64.b64encode(key).decode()}\n")

    # ── Parcours complet sur une racine factice ──────────────────────────────
    root, bindir, log = work / "root", work / "bin", work / "log"
    (root / "etc/winlux").mkdir(parents=True)
    (root / "usr/share/winlux").mkdir(parents=True)
    bindir.mkdir()
    test_defs = json.loads(DEFS.read_text(encoding="utf-8"))
    next(c for c in test_defs["catalogs"] if c["id"] == "flathub")["fingerprint"] = expected
    (root / "usr/share/winlux/store-catalogs.json").write_text(json.dumps(test_defs))
    # apt-get factice : installe un flatpak factice (qui note ses arguments).
    spare = work / "spare"
    spare.mkdir()
    fake(spare, "flatpak", 'printf "flatpak %s\\n" "$*" >> "$LOG"')
    fake(bindir, "apt-get", f'printf "apt-get %s\\n" "$*" >> "$LOG"\ncase "$*" in *install*) cp "{spare}/flatpak" "{bindir}/flatpak" ;; esac')
    fake(bindir, "update-desktop-database", "exit 0")
    base_env = {**os.environ, "WINLUX_CONTROL_ROOT": str(root), "PATH": f"{bindir}:/usr/bin:/bin",
                "LOG": str(log), "WINLUX_LANG": "fr", "WINLUX_FLATHUB_REPO": repo.as_uri().replace("file://", "file:")}

    def tool(*args, offline=False, lang="fr"):
        env = {**base_env, "WINLUX_LANG": lang}
        if offline: env["WINLUX_STORE_OFFLINE"] = "1"
        return subprocess.run([sys.executable, "-B", str(TOOL), *args], env=env, capture_output=True, text=True)

    def states(lang="fr"):
        return {row.split("|")[0]: row.split("|") for row in tool("--list", lang=lang).stdout.splitlines()}

    listing = states()
    ok(list(listing) == ["flathub", "scoop-extras", "scoop-games"], "liste en ordre alphabétique")
    ok(all(row[1] == "off" for row in listing.values()), "aucune boutique active d'office")
    ok(states("de")["scoop-games"][4].startswith("Freie und kostenlose"), "résumé dans la langue de l'interface (allemand)")

    # Sans réseau : Flathub en attente, Scoop Games prêt tout de suite.
    result = tool("--set", "flathub", "scoop-games", offline=True)
    ok(result.returncode == 75, "sans réseau : ajout reporté (code 75)")
    listing = states()
    ok(listing["flathub"][1] == "pending" and listing["scoop-games"][1] == "on" and listing["scoop-extras"][1] == "off",
       "sans réseau : Flathub en attente, Scoop Games actif, Scoop Extras non choisi")
    launchers = root / "usr/local/share/applications"
    ok(not (launchers / "winlux-store-flathub.desktop").exists(), "sans réseau : pas de lanceur Flathub avant l'ajout réel")
    games = desktop(launchers / "winlux-store-scoop-games.desktop")
    ok(games["Exec"] == "/usr/bin/winlux-shell-app store windows:scoop-games/ Scoop_Games", "lanceur Scoop Games : Store limité à cette boutique")
    ok(games.get("X-Winlux-Pin") == "true" and all(games.get(f"Comment{s}") for s in ("", "[en]", "[es]", "[de]")),
       "lanceur épinglable, décrit dans les quatre langues")
    ok("pas de réseau" in result.stdout, "message clair quand le réseau manque")
    ok(not log.exists() or "apt-get" not in log.read_text(), "sans réseau : apt-get n'est pas lancé")

    # Démarrage suivant avec réseau : l'init reprend.
    result = tool("--resume")
    ok(result.returncode == 0, "reprise au démarrage suivant : réussie")
    ok("apt-get install -y --no-install-recommends flatpak" in log.read_text(), "Flatpak installé depuis les dépôts du système")
    saved = root / "etc/winlux/store-repos/flathub.flatpakrepo"
    ok(saved.read_text() == repo.read_text() and saved.stat().st_mode & 0o777 == 0o644, "dépôt Flathub vérifié et gardé pour tous les comptes")
    ok(states()["flathub"][1] == "on" and (launchers / "winlux-store-flathub.desktop").is_file(), "Flathub actif et dans Toutes les applications")
    ok((root / "etc/winlux/store-catalogs-pending").read_text() == "", "plus rien en attente")
    ok(tool("--resume").returncode == 0, "reprise sans rien en attente : sans effet")

    # Clé différente : boutique refusée et laissée en attente.
    shutil.rmtree(root / "etc/winlux/store-repos")
    (launchers / "winlux-store-flathub.desktop").unlink()
    bad = json.loads((root / "usr/share/winlux/store-catalogs.json").read_text())
    next(c for c in bad["catalogs"] if c["id"] == "flathub")["fingerprint"] = "0" * 40
    (root / "usr/share/winlux/store-catalogs.json").write_text(json.dumps(bad))
    (root / "etc/winlux/store-catalogs-pending").write_text("flathub\n")
    result = tool("--resume")
    ok(result.returncode == 1 and "ne correspond pas" in result.stdout, "clé de signature différente : boutique refusée")
    ok(not saved.exists() and not (launchers / "winlux-store-flathub.desktop").exists(), "clé refusée : rien n'est gardé ni affiché")
    (root / "usr/share/winlux/store-catalogs.json").write_text(json.dumps(test_defs))
    ok(tool("--resume").returncode == 0 and saved.is_file(), "clé correcte au démarrage suivant : boutique ajoutée")

    # Retrait : lanceur et dépôt supprimés, applications installées gardées.
    result = tool("--remove", "flathub")
    ok(result.returncode == 0 and not saved.exists() and not (launchers / "winlux-store-flathub.desktop").exists(),
       "retrait de Flathub : lanceur et dépôt supprimés")
    ok(states()["flathub"][1] == "off" and states()["scoop-games"][1] == "on", "retrait : les autres boutiques restent")
    ok(tool("--add", "inconnue").returncode == 2, "boutique inconnue : refusée")
    ok(tool("--set").returncode == 0 and all(r[1] == "off" for r in states().values()), "« Choisir plus tard » : plus aucune boutique")

    # ── Le Store ne montre que les boutiques choisies ───────────────────────
    home = work / "home"
    home.mkdir()
    store_env = {"WINLUX_CONTROL_ROOT": str(root), "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
                 "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache")}
    saved_env = {k: os.environ.get(k) for k in store_env}
    os.environ.update(store_env)
    try:
        store = runpy.run_path(str(SCRIPTS / "winlux-store.py"), run_name="winlux_store")
        windows = store["Windows"]()
        ok([p.id for p in windows.providers()] == [], "Store : aucun catalogue Windows sans choix")
        (root / "etc/winlux/store-catalogs").write_text("scoop-games\n")
        providers = windows.providers()
        ok([p.id for p in providers] == ["scoop-games"] and providers[0].conf["format"] == "scoop", "Store : Scoop Games choisi, lu au format Scoop")
        ok(not store["AppImage"]().enabled(), "Store : AppImageHub (ni examiné ni signé) masqué par défaut")
        (root / "etc/winlux/store-sources.d").mkdir(parents=True)
        (root / "etc/winlux/store-sources.d/scoop-extras.json").write_text(json.dumps(
            {"id": "scoop-extras", "format": "scoop", "url": "https://example.org/x.tar.gz"}))
        ok(sorted(p.id for p in windows.providers()) == ["scoop-extras", "scoop-games"],
           "installation existante : son catalogue Scoop Extras d'avant reste actif")
        # Dépôt Flathub déclaré pour le compte, sans réseau ni droits.
        (root / "etc/winlux/store-catalogs").write_text("flathub\n")
        (root / "etc/winlux/store-repos").mkdir(exist_ok=True)
        (root / "etc/winlux/store-repos/flathub.flatpakrepo").write_text(repo.read_text())
        old_path = os.environ["PATH"]
        os.environ["PATH"] = f"{bindir}:{old_path}"
        os.environ["LOG"] = str(log)
        log.write_text("")
        store["Flatpak"]().user_remotes()
        os.environ["PATH"] = old_path
        ok(f"flatpak remote-add --user --if-not-exists --from flathub {root}/etc/winlux/store-repos/flathub.flatpakrepo" in log.read_text(),
           "Store : Flathub déclaré pour le compte (flatpak --user)")
    finally:
        for k, v in saved_env.items():
            if v is None: os.environ.pop(k, None)
            else: os.environ[k] = v

# ── Lanceur principal du Store ──────────────────────────────────────────────
entry = desktop(ROOT / "integration/native/applications/winlux-store.desktop")
ok((entry["Name"], entry["Name[en]"], entry["Name[es]"], entry["Name[de]"]) ==
   ("Store Winlux", "Winlux Store", "Tienda Winlux", "Winlux Store"), "Store dans Toutes les applications, nommé dans les quatre langues")
ok(entry["Exec"] == "/usr/bin/winlux-shell-app store" and "Settings" not in entry["Categories"], "Store : lance l'application Store, pas caché comme un réglage")
ok("Boutique" in entry["Keywords"] and all(entry.get(f"Keywords[{l}]") for l in ("en", "es", "de")), "Store trouvé en tapant « boutique » (mots-clés en 4 langues)")

# Menu Démarrer : épingle par défaut, épinglé une fois pour un compte existant.
source = (SCRIPTS / "winlux-start-menu.py").read_text(encoding="utf-8")
tree = ast.parse(source)
wanted = {"fold", "matches", "auto_pin"}
nodes = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in wanted
         or isinstance(n, ast.Assign) and any(getattr(t, "id", "") in ("DEFAULTS", "_LIGATURES") for t in n.targets)]
space = {"unicodedata": unicodedata}
exec(compile(ast.Module(body=nodes, type_ignores=[]), "winlux-start-menu.py", "exec"), space)
ok("winlux-store.desktop" in space["DEFAULTS"], "Store épinglé au menu Démarrer d'un compte neuf")


class App:
    def __init__(self, name, keywords=(), pin=False):
        self.name, self.keywords, self.pin = name, list(keywords), pin
    def get_display_name(self): return self.name
    def get_keywords(self): return self.keywords
    def has_key(self, key): return key == "X-Winlux-Pin" and self.pin
    def get_boolean(self, key): return self.pin


apps = {"winlux-store.desktop": App("Store Winlux", ["Boutique", "Logiciels"], True),
        "winlux-store-flathub.desktop": App("Flathub", ["Boutique"], True), "geany.desktop": App("Geany")}
state = {"pins": ["winlux-explorer.desktop"]}
ok(space["auto_pin"](state, apps) and state["pins"] == ["winlux-explorer.desktop", "winlux-store.desktop", "winlux-store-flathub.desktop"],
   "compte existant : Store et boutique choisie épinglés")
state["pins"].remove("winlux-store-flathub.desktop")
ok(not space["auto_pin"](state, apps) and "winlux-store-flathub.desktop" not in state["pins"], "détachée par l'utilisateur : elle le reste")
del apps["winlux-store-flathub.desktop"]
space["auto_pin"](state, apps)
apps["winlux-store-flathub.desktop"] = App("Flathub", [], True)
space["auto_pin"](state, apps)
ok("winlux-store-flathub.desktop" in state["pins"], "boutique retirée puis choisie de nouveau : épinglée de nouveau")
ok(space["matches"](apps["winlux-store.desktop"], space["fold"]("boutique")) and not space["matches"](apps["geany.desktop"], "boutique"),
   "recherche du menu Démarrer par mot-clé")

# ── Raccordements : image, init, relais root, installateur, Paramètres ──────
build = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
ok('"$stage/usr/bin/winlux-store-catalogs"' in build and '"$stage/usr/share/winlux/store-catalogs.json"' in build,
   "image : outil des boutiques et leur liste copiés")
ok("winlux-store-catalogs --resume" in build and "store-catalogs-pending" in build, "init : boutiques en attente reprises au démarrage")
policy = runpy.run_path(str(SCRIPTS / "winlux-root.py"), run_name="winlux_root_lib")["control_level"]
ok(policy(["store-catalog-add-flathub"]) == "admin" and policy(["store-catalog-remove-scoop-games"]) == "admin",
   "Paramètres : ajouter ou retirer une boutique demande l'autorisation administrateur")
control = (SCRIPTS / "winlux-control.sh").read_text(encoding="utf-8")
ok("store-catalog-add-*|store-catalog-remove-*)" in control, "winlux-control relaie l'ajout et le retrait")
installer = (SCRIPTS / "winlux-installer.py").read_text(encoding="utf-8")
ok(installer.count('"store_title":') == 6 and installer.count('"summary_store_none":') == 6, "installateur : choix des boutiques dans ses six langues")
ok('"store_catalogs": [],' in installer and "store-catalogs-pending" in installer, "installateur : rien de coché, choix repris au premier démarrage")
ok("store_catalogs" in (SCRIPTS / "winlux_autoinstall.py").read_text(), "fichier de réponse : clé store_catalogs")
oobe = (SCRIPTS / "winlux-first-boot.sh").read_text(encoding="utf-8")
ok("winlux-store-catalogs --set" in oobe, "premier démarrage : choix des boutiques")
pages = (SCRIPTS / "winlux-settings-pages.py").read_text(encoding="utf-8")
ok("self.store_catalogs_section()" in pages and "store-catalog-" in pages, "Paramètres > Applications : boutiques activables")
welcome = (SCRIPTS / "winlux-welcome.py").read_text(encoding="utf-8")
ok('"Choisir des boutiques de logiciels"' in welcome, "Bienvenue : bouton vers le choix des boutiques")
shell = subprocess.run(["sh", str(SCRIPTS / "winlux-shell-app.sh"), "store", "bad;rm", "x"], capture_output=True)
ok(shell.returncode == 2, "lanceur : filtre de boutique invalide refusé")

# ── Hors ligne : un message humain partout, jamais une trace technique ─────
ok("test \"$result\" = 75" in control and "Pas de connexion Internet" in control, "winlux-control : code 75 = « Pas de connexion Internet », pas « Échec »")
settings = (SCRIPTS / "winlux-settings.py").read_text(encoding="utf-8")
ok('result == 75 and action and action.startswith(("browser-", "store-catalog-"))' in settings, "Paramètres : information (et non erreur) quand le réseau manque")
ok(re.search(r"'Pas de connexion Internet': \('No Internet connection', 'Sin conexión a Internet', 'Keine Internetverbindung'\)", settings),
   "Paramètres : message hors ligne dans les quatre langues")
ok("browsers_waiting()" in settings, "Paramètres > Navigateurs : navigateur en attente d'Internet affiché")
ok(installer.count('"summary_offline_later":') == 6, "installateur : récapitulatif sans réseau dans ses six langues")
ok("browser-install-pending" in build, "init : navigateur demandé hors ligne installé au démarrage connecté")
store_src = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
ok('elif not network_up():' in store_src, "Store : échec sans réseau expliqué simplement")

print(f"OK : {checks} vérifications des boutiques de logiciels")
