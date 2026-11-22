#!/usr/bin/python3 -I
"""Mise à jour de la Boutique depuis GitHub, sans mot de passe (#3133).

    winlux-boutique-maj etat
    winlux-boutique-maj rechercher [--auto]
    winlux-boutique-maj installer
    winlux-boutique-maj precedente | livree
    winlux-boutique-maj auto oui|non

La Boutique publiée par site.yml vit à une adresse fixe (GitHub Pages) :
MANIFESTE, MANIFESTE.sig et les fichiers qu'il nomme. Le manifeste donne :
  - un numéro monotone, le nombre de commits de master en first-parent ;
  - le commit ;
  - la version du lanceur attendue ;
  - pour chaque fichier, son SHA-256, sa taille et son nom.
Il est signé en Ed25519 avec la clé propre à la Boutique. Sa clé publique
est /usr/share/winlux/store-signing.pub. Ce n'est pas la clé des mises à
jour de l'OS.

« rechercher » ne télécharge que le manifeste et sa signature. Avec --auto
(ouverture de la Boutique), rien ne se passe si le réglage « Rechercher au
lancement » est coupé ou si la dernière recherche date de moins de 6 h.
Une version nouvelle n'est signalée que par une bulle, une seule fois.

« installer » affiche d'abord la confirmation de tout téléchargement : paquet,
version, éditeur, source et SHA-256 du manifeste, avec Annuler par défaut.
La pose est refusée si :
  - la signature est fausse ou absente ;
  - un SHA-256 ou une taille ne correspond pas ;
  - le numéro ne dépasse pas celui de la copie active ou livrée.
La pose se fait dans un dossier de préparation, renommé ensuite, puis le
lien « actuelle » est remplacé d'un coup. La version d'avant reste
disponible pour « precedente ». « livree » rend la main à la copie du
système. Ces deux retours en arrière sont le choix de l'utilisateur, donc
permis même vers un numéro plus ancien.

Sortie : lignes clé=valeur. « resultat= » vaut l'un de :
a_jour, disponible, pose, annule, hors_ligne, signature, cle_absente,
empreinte, ancien, invalide, aucune, fait, coupe.
Le code de sortie est 0 pour a_jour, disponible, pose, fait et coupe,
1 pour annule, 3 pour les refus et les pannes.
"""
import fcntl
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

SOURCE = os.environ.get("WINLUX_BOUTIQUE_SOURCE", "https://univers4craft.github.io/winlux/boutique/")
CLE = Path(os.environ.get("WINLUX_BOUTIQUE_CLE", "/usr/share/winlux/store-signing.pub"))
SIGNATURE = os.environ.get("WINLUX_SIGNATURE", "/usr/bin/winlux-signature")
CONFIRM = os.environ.get("WINLUX_STORE_CONFIRM_LIVRE", "/usr/lib/winlux/winlux-store-confirm")
LANCEUR = Path(os.environ.get("WINLUX_STORE_LANCEUR", "/usr/bin/winlux-store"))
DELAI = 10
DELAI_RECHERCHE = 6 * 3600
TAILLE_MANIFESTE = 65536
TAILLE_FICHIER = 8 * 1024 * 1024
EDITEUR = "Univers4craft"

_LANGUE = (os.environ.get("LANGUAGE") or os.environ.get("LC_ALL") or os.environ.get("LC_MESSAGES")
           or os.environ.get("LANG") or "fr")[:2]
_TR = {
    "Store Winlux": ("Winlux Store", "Tienda Winlux", "Winlux-Store"),
    "Mettre à jour le Store ?": ("Update the Store?", "¿Actualizar la Tienda?", "Store aktualisieren?"),
    "Installer": ("Install", "Instalar", "Installieren"),
    "Annuler": ("Cancel", "Cancelar", "Abbrechen"),
    "Paquet": ("Package", "Paquete", "Paket"),
    "Version": ("Version", "Versión", "Version"),
    "Éditeur": ("Publisher", "Editor", "Herausgeber"),
    "Source": ("Source", "Origen", "Quelle"),
    "Empreinte SHA-256 du manifeste signé": ("SHA-256 of the signed manifest", "SHA-256 del manifiesto firmado",
                                             "SHA-256 des signierten Manifests"),
    "Mise à jour du Store disponible": ("Store update available", "Actualización de la Tienda disponible",
                                              "Store-Update verfügbar"),
    "Version {n}. Pour l’installer : Paramètres > Applications > Boutiques de logiciels.": (
        "Version {n}. To install it: Settings > Apps > Software stores.",
        "Versión {n}. Para instalarla: Configuración > Aplicaciones > Tiendas de programas.",
        "Version {n}. Zum Installieren: Einstellungen > Apps > Software-Stores."),
}


def _t(texte):
    ligne = _TR.get(texte)
    return ligne[("en", "es", "de").index(_LANGUE)] if ligne and _LANGUE in ("en", "es", "de") else texte


def dossier():
    base = os.environ.get("WINLUX_BOUTIQUE_DIR")
    if base:
        return Path(base)
    data = os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local/share")
    return Path(data) / "winlux/boutique"


def fichier_reglages():
    config = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(config) / "winlux/boutique-maj.conf"


def lanceur():
    """Le lanceur installé : ses règles (numéro livré, contrôle) sont les seules."""
    chemin = str(LANCEUR)
    chargeur = importlib.machinery.SourceFileLoader("winlux_store_lanceur", chemin)
    spec = importlib.util.spec_from_loader("winlux_store_lanceur", chargeur)
    module = importlib.util.module_from_spec(spec)
    chargeur.exec_module(module)
    return module


def signature():
    chargeur = importlib.machinery.SourceFileLoader("winlux_signature", SIGNATURE)
    spec = importlib.util.spec_from_loader("winlux_signature", chargeur)
    module = importlib.util.module_from_spec(spec)
    chargeur.exec_module(module)
    return module


def sortie(resultat, **cles):
    print(f"resultat={resultat}")
    for cle, valeur in cles.items():
        print(f"{cle}={valeur}")
    if resultat in ("a_jour", "disponible", "pose", "fait", "coupe"):
        return 0
    return 1 if resultat == "annule" else 3


def preparer_dossier():
    base = dossier()
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(base, 0o700)
    return base


def verrou(base):
    f = open(base / ".verrou", "a")
    fcntl.flock(f, fcntl.LOCK_EX)
    return f


class TropGros(Exception):
    """Fichier plus long que la taille annoncée : traité comme une empreinte fausse."""


def telecharger(nom, limite):
    url = urllib.parse.urljoin(SOURCE, nom)
    if not url.startswith("https://") and not os.environ.get("WINLUX_BOUTIQUE_TEST"):
        raise ValueError("source non HTTPS")

    class SeulementHttps(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            if not newurl.startswith("https://") and not os.environ.get("WINLUX_BOUTIQUE_TEST"):
                raise urllib.error.URLError("redirection hors HTTPS refusée")
            return super().redirect_request(req, fp, code, msg, headers, newurl)

    ouvreur = urllib.request.build_opener(SeulementHttps)
    requete = urllib.request.Request(url, headers={"User-Agent": "winlux-boutique-maj", "Cache-Control": "no-cache"})
    with ouvreur.open(requete, timeout=DELAI) as reponse:
        donnees = reponse.read(limite + 1)
    if len(donnees) > limite:
        raise TropGros(f"{nom} dépasse {limite} octets")
    return donnees


def lire_manifeste(texte):
    cles, fichiers = {}, []
    for ligne in texte.decode("utf-8").splitlines():
        cle, _, valeur = ligne.partition("=")
        if cle == "fichier":
            fichiers.append(valeur)
        elif cle:
            cles[cle] = valeur
    cles["fichier"] = fichiers
    return cles


def manifeste_distant():
    """(cles, octets du manifeste, octets de la signature) ou un résultat de refus."""
    if not CLE.is_file():
        return "cle_absente"
    try:
        manifeste = telecharger("MANIFESTE", TAILLE_MANIFESTE)
        sig = telecharger("MANIFESTE.sig", 1024)
    except TropGros:
        return "invalide"
    except (OSError, ValueError, urllib.error.URLError):
        return "hors_ligne"
    try:
        outil = signature()
        bonne = outil.verify(outil.read_hex(CLE, 32, "clé publique"), manifeste,
                             bytes.fromhex(sig.decode("ascii").strip()))
    except (OSError, ValueError, UnicodeDecodeError):
        bonne = False
    if not bonne:
        return "signature"
    try:
        cles = lire_manifeste(manifeste)
    except UnicodeDecodeError:
        return "invalide"
    lance = lanceur()
    if cles.get("boutique") != "1" or lance.numero(cles) < 1 or not cles["fichier"]:
        return "invalide"
    for entree in cles["fichier"]:
        parts = entree.split(" ", 2)
        if (len(parts) != 3 or parts[2] not in lance.NOMS or len(parts[0]) != 64
                or not parts[1].isdigit() or int(parts[1]) > TAILLE_FICHIER):
            return "invalide"
    return cles, manifeste, sig


def numeros(base):
    """(numéro actif, numéro livré) : actif vaut 0 sans copie du compte."""
    lance = lanceur()
    livree = lance.numero_livree()
    actif = 0
    try:
        cible = os.readlink(base / "actuelle")
        cles = lance.lire_cles(str(base / cible / "MANIFESTE"))
        if cible.isdigit() and cles:
            actif = lance.numero(cles)
    except OSError:
        pass
    return actif, livree


def bulle(n):
    titre = _t("Mise à jour du Store disponible")
    corps = _t("Version {n}. Pour l’installer : Paramètres > Applications > Boutiques de logiciels.").format(n=n)

    def chaine(s):
        return "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"
    gdbus = shutil.which("gdbus")
    if gdbus:
        commande = [gdbus, "call", "--session", "--timeout", "5", "--dest", "org.freedesktop.Notifications",
                    "--object-path", "/org/freedesktop/Notifications", "--method",
                    "org.freedesktop.Notifications.Notify", "--", chaine(_t("Store Winlux")), "0",
                    chaine("system-software-update"), chaine(titre), chaine(corps), "@as []", "@a{sv} {}", "-1"]
    elif shutil.which("notify-send"):
        commande = ["notify-send", "-a", _t("Store Winlux"), titre, corps]
    else:
        return
    try:
        subprocess.run(commande, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    except (OSError, subprocess.SubprocessError):
        pass


def rechercher(auto):
    if auto and not lanceur().reglage_auto():
        return sortie("coupe")
    base = preparer_dossier()
    marque = base / "derniere-recherche"
    if auto:
        try:
            if time.time() - marque.stat().st_mtime < DELAI_RECHERCHE:
                return sortie("coupe")
        except OSError:
            pass
    marque.touch()
    resultat = manifeste_distant()
    if isinstance(resultat, str):
        return sortie(resultat, source=SOURCE)
    cles, manifeste, _sig = resultat
    n = lanceur().numero(cles)
    actif, livree = numeros(base)
    if n <= max(actif, livree):
        return sortie("a_jour", numero=n, source=SOURCE)
    if auto:
        signale = base / "signalee"
        try:
            deja = signale.read_text(encoding="ascii").strip()
        except OSError:
            deja = ""
        if deja != str(n):
            bulle(n)
            signale.write_text(f"{n}\n", encoding="ascii")
    return sortie("disponible", numero=n, commit=cles.get("commit", ""), source=SOURCE,
                  sha256=hashlib.sha256(manifeste).hexdigest())


def confirmer(cles, manifeste):
    texte = "\n".join((
        f"{_t('Paquet')} : {_t('Store Winlux')}",
        f"{_t('Version')} : {cles.get('numero')} ({cles.get('commit', '')[:12]})",
        f"{_t('Éditeur')} : {EDITEUR}",
        f"{_t('Source')} : {SOURCE}",
        f"{_t('Empreinte SHA-256 du manifeste signé')} : {hashlib.sha256(manifeste).hexdigest()}",
    ))
    if os.environ.get("WINLUX_BOUTIQUE_SANS_CONFIRMATION") == "1" and os.environ.get("WINLUX_BOUTIQUE_TEST"):
        return True  # tests seulement : jamais dans une session réelle
    commande = [CONFIRM, "--title", _t("Mettre à jour le Store ?"), "--text", texte,
                "--ok", _t("Installer"), "--cancel", _t("Annuler")]
    if CONFIRM.endswith(".py"):
        commande = [sys.executable, "-I", *commande]
    try:
        return subprocess.run(commande, timeout=3600).returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def remplacer_lien(base, nom, cible):
    temporaire = base / f".{nom}.{os.getpid()}"
    try:
        temporaire.unlink()
    except FileNotFoundError:
        pass
    os.symlink(cible, temporaire)
    os.replace(temporaire, base / nom)


def lien(base, nom):
    try:
        cible = os.readlink(base / nom)
    except OSError:
        return ""
    return cible if cible.isdigit() else ""


def nettoyer(base):
    gardes = {lien(base, "actuelle"), lien(base, "precedente")} - {""}
    for entree in base.iterdir():
        if entree.name.isdigit() and entree.name not in gardes and not entree.is_symlink():
            shutil.rmtree(entree, ignore_errors=True)
        elif entree.name.startswith(".prep-"):
            shutil.rmtree(entree, ignore_errors=True)


def installer():
    resultat = manifeste_distant()
    if isinstance(resultat, str):
        return sortie(resultat, source=SOURCE)
    cles, manifeste, sig = resultat
    n = lanceur().numero(cles)
    base = preparer_dossier()
    actif, livree = numeros(base)
    if n <= max(actif, livree):
        return sortie("ancien" if n < max(actif, livree) else "a_jour", numero=n, source=SOURCE)
    if not confirmer(cles, manifeste):
        return sortie("annule", numero=n)
    with verrou(base):
        prep = Path(tempfile.mkdtemp(prefix=".prep-", dir=base))
        try:
            for entree in cles["fichier"]:
                empreinte, taille, nom = entree.split(" ", 2)
                try:
                    donnees = telecharger(nom, int(taille))
                except TropGros:
                    return sortie("empreinte", fichier=nom, source=SOURCE)
                except (OSError, ValueError, urllib.error.URLError):
                    return sortie("hors_ligne", fichier=nom, source=SOURCE)
                if len(donnees) != int(taille) or hashlib.sha256(donnees).hexdigest() != empreinte:
                    return sortie("empreinte", fichier=nom, source=SOURCE)
                (prep / nom).write_bytes(donnees)
                os.chmod(prep / nom, 0o755 if nom.endswith(".py") else 0o644)
            # Bytecode vérifié par l'empreinte de la source : le lanceur ne
            # recompile pas le Store à chaque commande (#3133).
            try:
                import py_compile
                py_compile.compile(str(prep / "winlux-store.py"), doraise=True,
                                   invalidation_mode=py_compile.PycInvalidationMode.CHECKED_HASH)
            except (py_compile.PyCompileError, OSError):
                pass  # une copie qui ne compile pas sera refusée au lancement
            (prep / "MANIFESTE").write_bytes(manifeste)
            (prep / "MANIFESTE.sig").write_bytes(sig)
            os.chmod(prep / "MANIFESTE", 0o644)
            os.chmod(prep / "MANIFESTE.sig", 0o644)
            os.chmod(prep, 0o755)
            final = base / str(n)
            if final.exists():
                shutil.rmtree(final)
            os.rename(prep, final)
        finally:
            shutil.rmtree(prep, ignore_errors=True)
        avant = lien(base, "actuelle")
        if avant and avant != str(n):
            remplacer_lien(base, "precedente", avant)
        remplacer_lien(base, "actuelle", str(n))
        retirer_echec(base, str(n))
        nettoyer(base)
    return sortie("pose", numero=n, commit=cles.get("commit", ""), source=SOURCE)


def retirer_echec(base, n):
    chemin = base / "echecs"
    try:
        lignes = [l for l in chemin.read_text(encoding="ascii").split() if l != n]
    except OSError:
        return
    chemin.write_text("".join(l + "\n" for l in lignes), encoding="ascii")


def precedente():
    base = preparer_dossier()
    with verrou(base):
        avant, actuelle = lien(base, "precedente"), lien(base, "actuelle")
        if not avant or not (base / avant).is_dir():
            return sortie("aucune")
        remplacer_lien(base, "actuelle", avant)
        if actuelle:
            remplacer_lien(base, "precedente", actuelle)
        else:
            (base / "precedente").unlink()
        retirer_echec(base, avant)
    return sortie("fait", numero=avant)


def livree():
    base = preparer_dossier()
    with verrou(base):
        actuelle = lien(base, "actuelle")
        if actuelle:
            remplacer_lien(base, "precedente", actuelle)
            (base / "actuelle").unlink()
        nettoyer(base)
    return sortie("fait")


def etat():
    base = dossier()
    lance = lanceur()
    actif, livree_n = numeros(base) if base.is_dir() else (0, lance.numero_livree())
    utilisee = lance.copie_du_compte()
    cles = {}
    if actif:
        cles = lance.lire_cles(str(base / str(actif) / "MANIFESTE")) or {}
    try:
        signale = (base / "signalee").read_text(encoding="ascii").strip()
    except OSError:
        signale = ""
    try:
        derniere = int((base / "derniere-recherche").stat().st_mtime)
    except OSError:
        derniere = 0
    print(f"auto={int(lance.reglage_auto())}")
    print(f"livree={livree_n}")
    print(f"compte={actif}")
    print(f"commit={cles.get('commit', '')}")
    print(f"utilisee={'compte' if utilisee else 'livree'}")
    print(f"precedente={lien(base, 'precedente') if base.is_dir() else ''}")
    print(f"echecs={' '.join(sorted(lance.echecs(str(base)))) if base.is_dir() else ''}")
    print(f"signalee={signale}")
    print(f"derniere_recherche={derniere}")
    print(f"source={SOURCE}")
    print(f"cle={int(CLE.is_file())}")
    return 0


def auto(valeur):
    chemin = fichier_reglages()
    chemin.parent.mkdir(parents=True, exist_ok=True)
    temporaire = chemin.with_name(chemin.name + ".tmp")
    temporaire.write_text(f"auto={'1' if valeur == 'oui' else '0'}\n", encoding="ascii")
    os.replace(temporaire, chemin)
    return sortie("fait", auto=int(valeur == "oui"))


def main(argv):
    if os.geteuid() == 0:
        print("winlux-boutique-maj : à lancer depuis le compte, jamais en root", file=sys.stderr)
        return 2
    action = argv[1] if len(argv) > 1 else ""
    if action == "etat" and len(argv) == 2:
        return etat()
    if action == "rechercher" and argv[2:] in ([], ["--auto"]):
        return rechercher(argv[2:] == ["--auto"])
    if action == "installer" and len(argv) == 2:
        return installer()
    if action == "precedente" and len(argv) == 2:
        return precedente()
    if action == "livree" and len(argv) == 2:
        return livree()
    if action == "auto" and argv[2:] in (["oui"], ["non"]):
        return auto(argv[2])
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
