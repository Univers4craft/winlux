#!/usr/bin/python3
"""Lanceur de la Boutique (#3133), installé en /usr/bin/winlux-store.

La Boutique livrée avec le système est /usr/lib/winlux/winlux-store.py. Une
version plus récente peut avoir été posée dans le compte par
winlux-boutique-maj, depuis GitHub, sans mot de passe :

    ~/.local/share/winlux/boutique/actuelle -> <numéro>/
        MANIFESTE, MANIFESTE.sig, winlux-store.py, store-*.json…

Le lanceur exécute cette copie seulement si toutes ces conditions tiennent :
  - le processus n'est pas root (root n'exécute jamais un fichier du compte) ;
  - le dossier et la copie appartiennent au compte, sans écriture pour
    le groupe ni pour les autres ;
  - le numéro du manifeste dépasse celui de la copie livrée, et la copie
    n'a pas déjà échoué à démarrer ;
  - chaque fichier a la taille et le SHA-256 que donne le manifeste.

La signature Ed25519 du manifeste est vérifiée à la pose, pas ici. Une
vérification à chaque lancement coûterait environ 18 ms en Python pur, et
elle ne protège de rien : seul ce même compte peut écrire dans le dossier.
Dans tous les autres cas, et si la copie du compte ne se charge pas (erreur
de syntaxe, module absent), la copie livrée est exécutée sans aucune
question (#1733). L'échec est noté pour que les Paramètres l'affichent.

À l'ouverture de la Boutique (commande « refresh »), une recherche de mise
à jour peut partir en arrière-plan, si le réglage visible « Rechercher au
lancement » est activé. Elle ne télécharge que le manifeste, au plus une
fois toutes les 6 heures, et n'affiche qu'une bulle (winlux-boutique-maj
rechercher --auto).
"""
import hashlib
import importlib.machinery
import importlib.util
import os
import stat
import sys

# Version du contrat entre ce lanceur et une Boutique publiée : un manifeste
# qui demande « lanceur=N » plus grand est ignoré (copie livrée).
LANCEUR = 1
NOMS = ("winlux-store.py", "winlux-store-confirm.py", "store-windows-valides.json",
        "store-editeurs.json", "store-catalog.txt")
# Fichier du compte -> variable que lit winlux-store.py.
VARIABLES = (("winlux-store-confirm.py", "WINLUX_STORE_CONFIRM"),
             ("store-windows-valides.json", "WINLUX_STORE_VALIDES"),
             ("store-editeurs.json", "WINLUX_STORE_EDITEURS"),
             ("store-catalog.txt", "WINLUX_STORE_CATALOG"))
LIVREE = os.environ.get("WINLUX_STORE_LIVREE", "/usr/lib/winlux/winlux-store.py")
LIVREE_NUMERO = os.environ.get("WINLUX_STORE_LIVREE_NUMERO", "/usr/share/winlux/boutique-livree.txt")
OUTIL = os.environ.get("WINLUX_BOUTIQUE_MAJ", "/usr/lib/winlux/winlux-boutique-maj")
DELAI_RECHERCHE = 6 * 3600


def dossier():
    base = os.environ.get("WINLUX_BOUTIQUE_DIR")
    if base:
        return base
    data = os.environ.get("XDG_DATA_HOME") or os.path.join(os.path.expanduser("~"), ".local/share")
    return os.path.join(data, "winlux/boutique")


def reglages():
    config = os.environ.get("XDG_CONFIG_HOME") or os.path.join(os.path.expanduser("~"), ".config")
    return os.path.join(config, "winlux/boutique-maj.conf")


def reglage_auto():
    """« Rechercher au lancement » : activé sauf « auto=0 » écrit par les Paramètres."""
    cles = lire_cles(reglages(), 4096)
    return not (cles and cles.get("auto") == "0")


def lire_cles(chemin, limite=65536):
    try:
        with open(chemin, "rb") as f:
            texte = f.read(limite + 1)
    except OSError:
        return None
    if len(texte) > limite:
        return None
    cles, fichiers = {}, []
    for ligne in texte.decode("utf-8", "replace").splitlines():
        cle, _, valeur = ligne.partition("=")
        if cle == "fichier":
            fichiers.append(valeur)
        elif cle:
            cles[cle] = valeur
    cles["fichier"] = fichiers
    return cles


def numero(cles):
    try:
        n = int(cles.get("numero", ""))
    except (ValueError, AttributeError):
        return -1
    return n if n >= 0 else -1


def numero_livree():
    cles = lire_cles(LIVREE_NUMERO, 4096)
    return numero(cles) if cles else 0


def sain(chemin, uid, dossier_attendu):
    """Appartient au compte, sans écriture pour le groupe ni les autres."""
    try:
        st = os.lstat(chemin)
    except OSError:
        return False
    if stat.S_ISLNK(st.st_mode):
        return False
    if dossier_attendu != stat.S_ISDIR(st.st_mode):
        return False
    return st.st_uid == uid and not st.st_mode & 0o022


def echecs(base):
    try:
        with open(os.path.join(base, "echecs"), encoding="ascii") as f:
            return {ligne.strip() for ligne in f if ligne.strip()}
    except (OSError, UnicodeDecodeError):
        return set()


def copie_du_compte():
    """Dossier de la copie du compte, ou None pour la copie livrée."""
    uid = os.geteuid()
    if uid == 0:
        return None
    base = dossier()
    if not sain(base, uid, True):
        return None
    try:
        cible = os.readlink(os.path.join(base, "actuelle"))
    except OSError:
        return None
    if not cible.isdigit():  # seulement un nom de dossier voisin, jamais un chemin
        return None
    copie = os.path.join(base, cible)
    if not sain(copie, uid, True) or cible in echecs(base):
        return None
    cles = lire_cles(os.path.join(copie, "MANIFESTE"))
    if not cles or numero(cles) != int(cible) or numero(cles) <= numero_livree():
        return None
    try:
        if int(cles.get("lanceur", "1")) > LANCEUR:
            return None
    except ValueError:
        return None
    vus = set()
    for entree in cles["fichier"]:
        parts = entree.split(" ", 2)
        if len(parts) != 3 or parts[2] not in NOMS:
            return None
        empreinte, taille, nom = parts
        chemin = os.path.join(copie, nom)
        if not sain(chemin, uid, False):
            return None
        try:
            with open(chemin, "rb") as f:
                contenu = f.read()
        except OSError:
            return None
        if str(len(contenu)) != taille or hashlib.sha256(contenu).hexdigest() != empreinte:
            return None
        vus.add(nom)
    if "winlux-store.py" not in vus:
        return None
    return copie, vus


def charger(chemin):
    """Charge le Store comme un module : son bytecode est gardé dans
    __pycache__ quand le dossier le permet. La copie du compte est compilée à
    la pose (pyc vérifié par empreinte de la source) : environ 34 ms de
    compilation évités à chaque commande du Store."""
    chargeur = importlib.machinery.SourceFileLoader("winlux_store", chemin)
    spec = importlib.util.spec_from_loader("winlux_store", chargeur)
    module = importlib.util.module_from_spec(spec)
    sys.modules["winlux_store"] = module
    chargeur.exec_module(module)
    return module


def noter_echec(numero_copie):
    try:
        with open(os.path.join(dossier(), "echecs"), "a", encoding="ascii") as f:
            f.write(numero_copie + "\n")
    except OSError:
        pass


def recherche_auto():
    """Recherche en arrière-plan : manifeste seul, bulle seule (#3133)."""
    if os.geteuid() == 0 or not os.access(OUTIL, os.X_OK):
        return
    # Deux stat et une lecture de quelques octets : rien ne part quand le
    # réglage est coupé ou qu'une recherche a eu lieu il y a moins de 6 h.
    if reglage_auto() is False:
        return
    try:
        import time
        if time.time() - os.stat(os.path.join(dossier(), "derniere-recherche")).st_mtime < DELAI_RECHERCHE:
            return
    except OSError:
        pass
    try:
        import subprocess
        subprocess.Popen([OUTIL, "rechercher", "--auto"], stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
    except OSError:
        pass


def main():
    if len(sys.argv) < 2 or sys.argv[1] == "refresh":
        recherche_auto()
    trouvee = copie_du_compte()
    if trouvee:
        copie, vus = trouvee
        for nom, variable in VARIABLES:
            if nom in vus and variable not in os.environ:
                os.environ[variable] = os.path.join(copie, nom)
        try:
            principal = charger(os.path.join(copie, "winlux-store.py")).main
        except Exception as erreur:  # copie illisible : la livrée prend le relais
            noter_echec(os.path.basename(copie))
            print(f"winlux-store : la Boutique du compte ({os.path.basename(copie)}) n'a pas démarré "
                  f"({type(erreur).__name__}) ; Boutique livrée utilisée", file=sys.stderr)
            for _nom, variable in VARIABLES:
                if os.environ.get(variable, "").startswith(copie + os.sep):
                    del os.environ[variable]
        else:
            return principal(sys.argv)
    return charger(LIVREE).main(sys.argv)


if __name__ == "__main__":
    sys.exit(main())
