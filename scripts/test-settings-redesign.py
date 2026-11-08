#!/usr/bin/python3
"""Refonte des pages Paramètres les plus visitées (#898).

Réseau : carte de connexion lisible (plus de « enp0s3 10.0.2.15/24 » brut),
hors ligne dit clairement, lignes vers les pages voisines seulement quand
elles existent. Comptes : carte d'en-tête du compte ouvert. Personnalisation :
aperçu du bureau aux couleurs du mode et de l'accentuation, fond et couleur
choisis marqués par un liseré. Textes nouveaux en quatre langues.
"""
from pathlib import Path
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


pages = runpy.run_path(str(HERE / "winlux-settings-pages.py"), run_name="redesign_test")
summary = pages["network_summary"]
with tempfile.TemporaryDirectory() as tmp:
    net = Path(tmp)
    (net / "wlp2s0/wireless").mkdir(parents=True)
    (net / "enp0s3").mkdir()
    title, line, glyph = summary({"network": "enp0s3 10.0.2.15/24", "network_connected": "1"}, net)
    check((title, glyph) == ("Ethernet", "ethernet") and "10.0.2.15" in line and "/24" not in line and "enp0s3" not in line,
          f"Ethernet lisible : {title!r} {line!r}")
    title, line, glyph = summary({"network": "wlp2s0 192.168.1.5/24", "network_connected": "1"}, net)
    check((title, glyph) == ("Wi-Fi", "wifi") and "192.168.1.5" in line, f"Wi-Fi reconnu : {title!r}")
    title, line, glyph = summary({"network": "Hors ligne - aucune route par defaut", "network_connected": "0"}, net)
    check(glyph == "offline" and title == "Hors ligne" and "defaut" not in line, f"hors ligne lisible : {line!r}")
    check(summary({}, net)[2] == "offline", "relevé absent : hors ligne")

# Pictogrammes et tuiles Prisme : SVG chargeables à la bonne taille.
for glyph in ("ethernet", "wifi", "offline", "vpn", "hotspot", "drive", "share", "user", "backup"):
    tile = pages["svg_pixbuf"](pages["tile_svg"](glyph, "#9b7bff", "#6a3df0"), 64)
    icon = pages["svg_pixbuf"](pages["line_svg"](glyph, "#6a3df0"), 24)
    check(tile.get_width() == 64 and icon.get_width() == 24, f"pictogramme {glyph}")

# Aperçu du bureau : barre des tâches et première icône aux couleurs choisies.
preview = pages["desktop_preview"](None, True, "#6a3df0")
check((preview.get_width(), preview.get_height()) == (240, 135), "aperçu 240×135")
pixels, stride, channels = preview.get_pixels(), preview.get_rowstride(), preview.get_n_channels()
def pixel(x, y):
    at = y * stride + x * channels
    return "#%02x%02x%02x" % tuple(pixels[at:at + 3])
check(pixel(240 // 2 - 26, 135 - 7) == "#6a3df0", f"icône à l'accentuation : {pixel(240 // 2 - 26, 135 - 7)}")
check(pixel(5, 135 - 3) == "#202027", "barre sombre en mode sombre")
light = pages["desktop_preview"](None, False, "#0078d4")
pixels, stride = light.get_pixels(), light.get_rowstride()
check(pixel(5, 135 - 3) == "#eceef3", "barre claire en mode clair")

# Textes nouveaux : anglais, espagnol, allemand.
tr = pages["_TR"]
for key in ("Ethernet", "Lecteurs réseau", "Partage et transfert", "Point d’accès sans fil mobile",
            "Renouveler la connexion", "adresse {address}", "Compte local", "Paramètres associés", "Couleur unie",
            "Aucune connexion. Branchez un câble réseau ou choisissez un réseau Wi-Fi.",
            "Ajouter, connecter et déconnecter un VPN.", "Partager la connexion de ce PC en Wi-Fi.",
            "Dossiers partagés d’autres PC, ouverts dans l’Explorateur.",
            "Partager un dossier, envoyer des fichiers à un PC proche.",
            "Copier vos documents sur un disque ou un dossier partagé."):
    check(len(tr.get(key, ())) == 3 and all(tr[key]), f"« {key} » sans ses trois traductions")

source = (HERE / "winlux-settings-pages.py").read_text()
settings = (HERE / "winlux-settings.py").read_text()
check('(_t("Connexion"), "network")' not in settings and '"network-renew")],' not in settings,
      "Réseau : plus de carte « Connexion » brute ni de bouton isolé")
check('self.execute("network-renew")' in source and "if self.page == 2: self.refresh_network_hero(status)" in settings,
      "Réseau : renouveler depuis la carte, carte relue chaque seconde")
check("page_available(row[0], self.status)" in source, "Réseau : pages voisines absentes non proposées")
check('self.hero("user", name, detail' in source, "Comptes : carte du compte ouvert")
check('add_class("chosen")' in source and "button.chosen" in settings and '"✓ " if str(path) == current' not in source,
      "fond et couleur choisis : liseré, plus de coche dans le titre")
check('self.label(crumb + "  ›" if fold(crumb) != fold(PAGES[self.current]) else " ", "subtitle")' in settings
      and "len(self.shown_pages(category)) > 1:\n            self.label(CATEGORIES" not in settings,
      "fil d'Ariane sur toutes les pages, titre toujours au même endroit (#904)")

# Navigateurs (#923) : confirmation avec éditeur, version, source et empreinte.
settings_module = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="redesign_browsers")
fetcher = runpy.run_path(str(HERE / "prepare-browser-packages.py"), run_name="redesign_fetch")
publishers, text = settings_module["BROWSER_PUBLISHERS"], settings_module["browser_download_text"]
check({ident: source for ident, (_, source) in publishers.items()} == {ident: row[1] for ident, row in fetcher["SOURCES"].items()},
      "sources affichées = dépôts réellement utilisés par le téléchargeur")
with tempfile.TemporaryDirectory() as tmp:
    (Path(tmp) / "var/cache/winlux/browsers").mkdir(parents=True)
    (Path(tmp) / "var/cache/winlux/browsers/firefox.version").write_text("147.0.1\n")
    shown = text("firefox", tmp)
    check("Mozilla" in shown and "147.0.1" in shown and "https://packages.mozilla.org/apt/" in shown and "SHA-256" in shown,
          f"confirmation Firefox : {shown!r}")
    shown = text("chrome", tmp)
    check("Google" in shown and "dernière version stable" in shown and "dl.google.com" in shown, f"version inconnue dite : {shown!r}")
tr = settings_module["_TR"]
for key in ("Installer {name} ?", "Télécharger et installer", "Annuler", "Éditeur : {publisher}", "Version : {version}",
            "Source : {source}", "dernière version stable publiée par l’éditeur"):
    check(len(tr.get(key, ())) == 3, f"« {key} » sans ses traductions")
check('button.connect("clicked", lambda _, value=action: self.confirm_browser(value))' in settings
      and "if response == Gtk.ResponseType.OK: self.execute(action)" in settings,
      "« Installer … » demande confirmation, refus possible, avant tout téléchargement")

# Menus jamais vides (#998) : politique d'affichage non choisie (display_policy=
# vide) = « follow » ; délai hors liste ajouté avec son libellé ; 4 langues.
check(settings_module["display_policy"]({"display_policy": ""}) == "follow"
      and settings_module["display_policy"]({}) == "follow"
      and settings_module["display_policy"]({"display_policy": "fixed"}) == "fixed", "politique d'affichage effective")
import os, subprocess
probe = r"""
import os, runpy, sys
m = runpy.run_path(sys.argv[1], run_name="probe")
Gtk = m["Gtk"]
box = Gtk.Box()
w = m["Settings"].__new__(m["Settings"])
combo = m["Settings"].choice_row(w, box, "x", [("follow", m["_t"]("Suivre la taille de la VM")), ("fixed", m["_t"]("Définition fixe"))], m["display_policy"]({"display_policy": ""}), "display-")
empty = m["Settings"].choice_row(w, box, "x", [("a", "A"), ("b", "B")], "", "p-")
delay = m["Settings"].choice_row(w, box, "x", [(s, m["delay_text"](s)) for s in m["POWER_DELAYS"]], 1200, "p-", m["delay_text"])
print(combo.get_active_text() or "", "|", empty.get_active_text() or "", "|", delay.get_active_text() or "")
"""
for lang in ("fr", "en", "es", "de"):
    env = dict(os.environ, WINLUX_LANG=lang, LANG=f"{lang}_FR.UTF-8" if lang == "fr" else "C.UTF-8", NO_AT_BRIDGE="1")
    # Vrais menus GTK : il faut un écran (CI : xvfb-run, comme les autres tests GTK).
    result = subprocess.run([sys.executable, "-c", probe, str(HERE / "winlux-settings.py")], env=env,
                            capture_output=True, text=True, timeout=60)
    out = result.stdout.strip().splitlines()
    parts = [p.strip() for p in (out[-1] if out else "").split("|")]
    check(len(parts) == 3 and all(parts), f"menu vide en {lang} : {parts} {result.stderr.strip()[-300:]}")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("OK : Réseau, Comptes et Personnalisation refaits (#898)")
