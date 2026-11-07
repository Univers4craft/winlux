#!/usr/bin/python3
"""Version précédente conservée : un seul avis dans Paramètres > Mises à jour
(#636), et pas de lanceur de session cassé dans les applications (#635).

    xvfb-run -a python3 -B scripts/test-update-previous-version.py
"""
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent

# ── #636 ─────────────────────────────────────────────────────────────────────
pages = runpy.run_path(str(HERE / "winlux-settings-pages.py"), run_name="previous_test")
kept = pages["previous_version_kept"]
files = {}
kept.__globals__["read"] = lambda path: files.get(path, "")
files["/usr/share/winlux/update-backend"] = "apt\n"
listing = "Version installée : v0.2.20\nWinlux est à jour.\nVersion précédente conservée : v0.2.19\n"
# Le compte ne voit pas /var/cache/winlux (cas du testeur) : la liste de root fait foi.
assert kept(listing) == "v0.2.19", kept(listing)
files["/run/winlux/updates.txt"] = listing
assert kept() == "v0.2.19"
assert kept("Version installée : v0.2.19\nWinlux est à jour.\n") == "", "liste sans version précédente"
files["/usr/share/winlux/update-backend"] = ""
files["/var/lib/winlux/rollback-current"] = "pas-une-sauvegarde"
assert kept() == "", "ancien installateur sans sauvegarde"
source = (HERE / "winlux-settings-pages.py").read_text(encoding="utf-8")
recovery = source[source.index("    def sections_recovery(self):"):]
recovery = recovery[:recovery.index("\n    # ──")]
assert "saved = previous_version_kept()" in recovery and "is_file()" not in recovery
settings = (HERE / "winlux-settings.py").read_text(encoding="utf-8")
assert settings.count("set_visible(bool(EXTRA.previous_version_kept()))") == 2 and "set_visible(Path(PREVIOUS_PACKAGE)" not in settings, \
    "bouton Restaurer : même avis que l'encadré"
print("#636 : encadré, bouton Restaurer et « Secours et restauration » disent la même chose")

# ── #635 ─────────────────────────────────────────────────────────────────────
build = (HERE / "build-live-iso.sh").read_text(encoding="utf-8")
assert 'cp "$root/scripts/winlux-session.desktop" "$stage/usr/share/xsessions/winlux.desktop"' in build
assert 'rm -f "$stage/usr/share/applications/winlux-session.desktop"' in build
assert '"$stage/usr/share/applications/winlux-session.desktop"' not in build.replace('rm -f "$stage/usr/share/applications/winlux-session.desktop"', "")
session = (HERE / "winlux-session.desktop").read_text(encoding="utf-8")
assert "Exec=/usr/share/winlux/xsession-winlux\n" in session, "Exec vers le vrai script de session"
assert 'cp "$root/scripts/xsession-winlux" "$stage/usr/share/winlux/xsession-winlux"' in build
apt = runpy.run_path(str(HERE / "stage-apt-root.py"), run_name="previous_test")
assert apt["winlux_path"]("usr/share/xsessions/winlux.desktop") and not apt["winlux_path"]("usr/share/xsessions/openbox.desktop")
print("#635 : session dans /usr/share/xsessions, plus de lanceur « Winlux Desktop » dans Démarrer")
