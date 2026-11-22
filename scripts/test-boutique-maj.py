#!/usr/bin/python3
"""Store mis à jour depuis GitHub sans mot de passe (#3133).

Un serveur HTTP local tient le rôle de GitHub Pages. La clé de test est
créée pour l'occasion. Le test vérifie, dans l'ordre :
  - pose acceptée, puis utilisée par le lanceur ;
  - refus d'une signature fausse, d'un SHA-256 faux, d'un numéro plus
    ancien ou égal ;
  - hors ligne : rien ne change ;
  - Annuler par défaut, et une confirmation qui nomme paquet, version,
    éditeur, source et empreinte ;
  - fichier altéré après la pose et copie qui ne démarre pas : la copie
    livrée prend le relais ;
  - root et droits trop larges : la copie du compte est ignorée ;
  - « Rechercher au lancement » : manifeste seul, une bulle, rien quand le
    réglage est coupé ou qu'une recherche date de moins de 6 h ;
  - retour à la version précédente puis à la version livrée ;
  - coût du lanceur.
"""
import hashlib
import http.server
import importlib.machinery
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
LANCEUR = HERE / "winlux-store-lanceur.py"
OUTIL = HERE / "winlux-boutique-maj.py"
SIGNATURE = HERE / "winlux-signature.py"
echecs = []


def ok(cond, libelle):
    print(("ok   " if cond else "ÉCHEC ") + libelle, flush=True)
    if not cond:
        echecs.append(libelle)


def charger(chemin, nom):
    chargeur = importlib.machinery.SourceFileLoader(nom, str(chemin))
    spec = importlib.util.spec_from_loader(nom, chargeur)
    module = importlib.util.module_from_spec(spec)
    chargeur.exec_module(module)
    return module


# En root, le lanceur ignore toujours la copie du compte : le test se relance
# sous « nobody », depuis une copie des scripts lisible par ce compte.
if os.geteuid() == 0 and not os.environ.get("WINLUX_TEST_RELANCE"):
    copie = Path(tempfile.mkdtemp(prefix="boutique-maj-scripts-", dir="/var/tmp"))
    for f in (Path(__file__).resolve(), LANCEUR, OUTIL, SIGNATURE):
        shutil.copy(f, copie / f.name)
    os.chmod(copie, 0o755)
    for f in copie.iterdir():
        os.chmod(f, 0o644)
    try:
        r = subprocess.run(["runuser", "-u", "nobody", "--", "env", "WINLUX_TEST_RELANCE=1", "TMPDIR=/var/tmp",
                            sys.executable, "-B", str(copie / Path(__file__).name)], cwd="/")
    finally:
        shutil.rmtree(copie, ignore_errors=True)
    sys.exit(r.returncode)

sig = charger(SIGNATURE, "winlux_signature")
tmp = Path(tempfile.mkdtemp(prefix="boutique-maj-"))
pages = tmp / "pages"
pages.mkdir()
compte = tmp / "compte"
(compte / "data").mkdir(parents=True)
(compte / "config").mkdir(parents=True)
graine = os.urandom(32)
(tmp / "store-signing.pub").write_text(sig.public_key(graine).hex() + "\n")
autre = os.urandom(32)
journal_confirm = tmp / "confirm.txt"
journal_bulle = tmp / "bulle.txt"
bin_dir = tmp / "bin"
bin_dir.mkdir()

# Store livré factice : il dit qui il est.
livree = tmp / "livree.py"
livree.write_text('import os, sys\ndef main(argv):\n    print("store=livree")\n    return 0\n')
(tmp / "boutique-livree.txt").write_text("numero=100\ncommit=\n")

# Confirmation factice : note ses arguments, répond selon REPONSE (Annuler sinon).
confirm = tmp / "confirm.py"
confirm.write_text(
    "import os, sys\n"
    f"open({str(journal_confirm)!r}, 'a').write(repr(sys.argv[1:]) + '\\n')\n"
    "sys.exit(0 if os.environ.get('REPONSE') == 'ok' else 1)\n")
# gdbus factice : note la bulle.
gdbus = bin_dir / "gdbus"
gdbus.write_text(f"#!/bin/sh\necho \"$@\" >> {journal_bulle}\n")
gdbus.chmod(0o755)


def publier(n, store_source=None, cle=graine, alterer=None, remplacer=None, extra=None):
    """Publie le numéro n sur le faux Pages, signé avec `cle`."""
    for f in pages.iterdir():
        f.unlink()
    store = store_source or (
        'import os, sys\ndef main(argv):\n'
        f'    print("store=compte{n}")\n'
        '    print("valides=" + os.environ.get("WINLUX_STORE_VALIDES", ""))\n'
        '    return 0\n')
    fichiers = {"winlux-store.py": store.encode(), "store-windows-valides.json": b'{"programmes": []}\n'}
    lignes = ["boutique=1", f"numero={n}", f"commit={'%040x' % n}", "lanceur=1"]
    for nom, donnees in fichiers.items():
        (pages / nom).write_bytes(donnees)
        lignes.append(f"fichier={hashlib.sha256(donnees).hexdigest()} {len(donnees)} {nom}")
    lignes += extra or []
    manifeste = ("\n".join(lignes) + "\n").encode()
    (pages / "MANIFESTE").write_bytes(manifeste)
    (pages / "MANIFESTE.sig").write_text(sig.sign(cle, manifeste).hex() + "\n")
    if alterer:  # un octet de plus
        (pages / alterer).write_bytes((pages / alterer).read_bytes() + b"#")
    if remplacer:  # même taille, un octet changé
        donnees = bytearray((pages / remplacer).read_bytes())
        donnees[-2] ^= 1
        (pages / remplacer).write_bytes(bytes(donnees))


requetes = []


class Pages(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(pages), **k)

    def log_message(self, *a):
        requetes.append(self.path)


serveur = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Pages)
threading.Thread(target=serveur.serve_forever, daemon=True).start()
port = serveur.server_address[1]

ENV = dict(os.environ,
           HOME=str(compte), XDG_DATA_HOME=str(compte / "data"), XDG_CONFIG_HOME=str(compte / "config"),
           PATH=f"{bin_dir}:{os.environ.get('PATH', '/usr/bin:/bin')}",
           WINLUX_BOUTIQUE_TEST="1", WINLUX_BOUTIQUE_SOURCE=f"http://127.0.0.1:{port}/boutique/",
           WINLUX_BOUTIQUE_CLE=str(tmp / "store-signing.pub"), WINLUX_SIGNATURE=str(SIGNATURE),
           WINLUX_STORE_LANCEUR=str(LANCEUR), WINLUX_STORE_LIVREE=str(livree),
           WINLUX_STORE_LIVREE_NUMERO=str(tmp / "boutique-livree.txt"),
           WINLUX_STORE_CONFIRM_LIVRE=str(confirm), WINLUX_BOUTIQUE_MAJ="/nonexistent")
for cle in ("WINLUX_STORE_VALIDES", "WINLUX_STORE_CONFIRM", "WINLUX_STORE_EDITEURS", "WINLUX_STORE_CATALOG",
            "WINLUX_BOUTIQUE_DIR", "LANGUAGE", "LC_ALL", "LC_MESSAGES"):
    ENV.pop(cle, None)
ENV["LANG"] = "fr_FR.UTF-8"
Pages.directory = str(pages)


class PagesBoutique(Pages):
    def translate_path(self, path):
        return str(pages / path.split("?")[0].rsplit("/", 1)[-1])


serveur.RequestHandlerClass = PagesBoutique
base = compte / "data/winlux/boutique"


def outil(*args, **env):
    r = subprocess.run([sys.executable, "-I", str(OUTIL), *args], env=dict(ENV, **env),
                       capture_output=True, text=True, timeout=60)
    sortie = dict(l.split("=", 1) for l in r.stdout.splitlines() if "=" in l)
    print(f"    winlux-boutique-maj {' '.join(args)} -> {r.returncode} {r.stdout.strip()!r} {r.stderr.strip()[-300:]!r}")
    return r.returncode, sortie


def lancer(*args):
    r = subprocess.run([sys.executable, str(LANCEUR), *(args or ("details", "x"))], env=ENV,
                       capture_output=True, text=True, timeout=60)
    print(f"    lanceur -> {r.returncode} {r.stdout.strip()!r} {r.stderr.strip()[-300:]!r}")
    return r.stdout


def actuelle():
    try:
        return os.readlink(base / "actuelle")
    except OSError:
        return ""


try:
    ok("store=livree" in lancer(), "sans copie du compte, le Store livré tourne (#3133)")

    publier(101)
    code, r = outil("rechercher")
    ok(code == 0 and r.get("resultat") == "disponible" and r.get("numero") == "101",
       "rechercher annonce la version 101 sans la poser (#3133)")
    ok(not actuelle() and requetes and all(p.endswith(("MANIFESTE", "MANIFESTE.sig")) for p in requetes),
       "rechercher ne télécharge que le manifeste et sa signature (#3133)")

    code, r = outil("installer")
    texte = journal_confirm.read_text() if journal_confirm.exists() else ""
    print("    confirmation : " + texte.strip())
    ok(code == 1 and r.get("resultat") == "annule" and not actuelle(), "Annuler par défaut : rien n'est posé (#3133)")
    ok(all(m in texte for m in ("Store Winlux", "101", "Univers4craft", f"127.0.0.1:{port}/boutique/",
                                hashlib.sha256((pages / "MANIFESTE").read_bytes()).hexdigest())),
       "la confirmation nomme paquet, version, éditeur, source et SHA-256 (#3133)")
    ok("'--cancel', 'Annuler'" in texte, "la confirmation propose Annuler (#3133)")

    code, r = outil("installer", REPONSE="ok")
    ok(code == 0 and r.get("resultat") == "pose" and actuelle() == "101", "une mise à jour acceptée est posée (#3133)")
    sortie = lancer()
    ok("store=compte101" in sortie and f"valides={base}/101/store-windows-valides.json" in sortie,
       "le lanceur exécute la copie du compte, avec son catalogue (#3133)")
    ok(oct((base / "101").stat().st_mode & 0o777) == "0o755" and oct(base.stat().st_mode & 0o777) == "0o700",
       "copie posée sans écriture pour le groupe ni les autres (#3133)")

    publier(102, cle=autre)
    code, r = outil("installer", REPONSE="ok")
    ok(code == 3 and r.get("resultat") == "signature" and actuelle() == "101",
       "une signature fausse est refusée net (#3133)")
    ok(r.get("source", "").startswith("http://127.0.0.1"), "le refus nomme la source (#3133)")

    publier(102, alterer="winlux-store.py")
    code, r = outil("installer", REPONSE="ok")
    ok(code == 3 and r.get("resultat") == "empreinte" and actuelle() == "101",
       "un SHA-256 faux est refusé, rien n'est posé (#3133)")
    publier(102, remplacer="store-windows-valides.json")
    code, r = outil("installer", REPONSE="ok")
    ok(code == 3 and r.get("resultat") == "empreinte" and r.get("fichier") == "store-windows-valides.json"
       and actuelle() == "101", "un SHA-256 faux à taille égale est refusé (#3133)")
    ok(not any(p.name.startswith(".prep-") or p.name == "102" for p in base.iterdir()),
       "aucun reste de préparation après un refus (#3133)")

    publier(99)
    code, r = outil("installer", REPONSE="ok")
    ok(code == 3 and r.get("resultat") == "ancien" and actuelle() == "101",
       "un numéro plus ancien est refusé (#3133)")
    publier(101)
    code, r = outil("rechercher")
    ok(r.get("resultat") == "a_jour", "le même numéro est « à jour » (#3133)")

    serveur.shutdown()
    serveur.server_close()
    code, r = outil("installer", REPONSE="ok")
    ok(code == 3 and r.get("resultat") == "hors_ligne" and actuelle() == "101",
       "hors ligne : la version courante est gardée (#3133)")
    serveur = http.server.ThreadingHTTPServer(("127.0.0.1", port), PagesBoutique)
    threading.Thread(target=serveur.serve_forever, daemon=True).start()

    publier(102)
    code, r = outil("installer", REPONSE="ok")
    ok(r.get("resultat") == "pose" and actuelle() == "102" and os.readlink(base / "precedente") == "101",
       "la version 102 est posée et la 101 gardée (#3133)")

    # Fichier altéré après la pose : la copie livrée tourne.
    store = base / "102/winlux-store.py"
    store.write_text(store.read_text() + "\n# altéré\n")
    ok("store=livree" in lancer(), "fichier altéré après la pose : Store livré (#3133)")
    code, r = outil("precedente")
    ok(r.get("resultat") == "fait" and actuelle() == "101" and "store=compte101" in lancer(),
       "retour à la version précédente (#3133)")

    # Droits trop larges : copie ignorée.
    os.chmod(base / "101", 0o775)
    ok("store=livree" in lancer(), "copie inscriptible par le groupe : Store livré (#3133)")
    os.chmod(base / "101", 0o755)

    # Root : jamais la copie du compte.
    module = charger(LANCEUR, "lanceur_root")
    os.environ.update({k: ENV[k] for k in ("XDG_DATA_HOME", "XDG_CONFIG_HOME", "WINLUX_STORE_LIVREE_NUMERO")})
    vrai = os.geteuid
    module.os.geteuid = lambda: 0
    ok(module.copie_du_compte() is None, "en root, la copie du compte est ignorée (#3133)")
    module.os.geteuid = vrai
    ok(module.copie_du_compte() is not None, "hors root, la même copie est retenue (#3133)")

    # Coût du lanceur : contrôle de la copie, mesuré en processus.
    tours = 200
    debut = time.perf_counter()
    for _ in range(tours):
        module.copie_du_compte()
    cout = (time.perf_counter() - debut) / tours * 1000
    taille = sum(p.stat().st_size for p in (base / "101").iterdir() if p.is_file())
    print(f"    coût du contrôle de la copie : {cout:.3f} ms ({taille} octets lus)")
    ok(cout < 2, f"le contrôle du lanceur coûte moins de 2 ms ({cout:.3f} ms) (#3133)")

    # Copie qui ne démarre pas : Store livré, échec noté, puis ignorée.
    publier(103, store_source="def main(argv:\n")
    code, r = outil("installer", REPONSE="ok")
    sortie = lancer()
    ok(r.get("resultat") == "pose" and "store=livree" in sortie, "copie qui ne démarre pas : Store livré (#3133)")
    code, r = outil("etat")
    ok(r.get("echecs") == "103" and r.get("utilisee") == "livree", "l'échec est noté et montré (#3133)")

    # Rechercher au lancement.
    publier(104)
    requetes.clear()
    outil("auto", "non")
    code, r = outil("rechercher", "--auto")
    ok(r.get("resultat") == "coupe" and not requetes, "réglage coupé : aucune requête (#3133)")
    outil("auto", "oui")
    os.utime(base / "derniere-recherche", (time.time() - 7 * 3600,) * 2)
    code, r = outil("rechercher", "--auto")
    bulles = journal_bulle.read_text().splitlines() if journal_bulle.exists() else []
    ok(r.get("resultat") == "disponible" and len(bulles) == 1 and "104" in bulles[0],
       "recherche au lancement : une bulle pour la version 104 (#3133)")
    ok(actuelle() == "103", "recherche au lancement : rien n'est posé (#3133)")
    requetes.clear()
    code, r = outil("rechercher", "--auto")
    ok(r.get("resultat") == "coupe" and not requetes, "moins de 6 h après : aucune requête (#3133)")
    os.utime(base / "derniere-recherche", (time.time() - 7 * 3600,) * 2)
    outil("rechercher", "--auto")
    bulles = journal_bulle.read_text().splitlines()
    ok(len(bulles) == 1, "la même version n'est signalée qu'une fois (#3133)")

    code, r = outil("livree")
    ok(r.get("resultat") == "fait" and not actuelle() and "store=livree" in lancer(),
       "retour à la version livrée (#3133)")
    code, r = outil("etat")
    ok(r.get("utilisee") == "livree" and r.get("precedente") == "103", "état après le retour (#3133)")
finally:
    serveur.shutdown()
    shutil.rmtree(tmp, ignore_errors=True)

print(f"{len(echecs)} échec(s)")
sys.exit(1 if echecs else 0)
