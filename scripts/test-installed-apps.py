#!/usr/bin/python3
"""Paramètres > Applications installées (#722) et doublon « Gestionnaire de
fichiers » retiré du menu Démarrer (#721).

Sans écran : liste construite sur une fausse base dpkg, de faux Flatpak et
runtime Windows, commandes de désinstallation (relais root pour APT), refus
des composants du système, quatre langues. Avec un écran (xvfb-run) : la
page elle-même, recherche, tri, filtres, boutons, largeur à 1366x768.
"""
import json
import os
from pathlib import Path
import re
import runpy
import stat
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
problems = []


def check(condition, message):
    if not condition:
        problems.append(message)
        print("ÉCHEC :", message)


def write(path, text, mode=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    if mode: path.chmod(mode)


work = Path(tempfile.mkdtemp(prefix="winlux-installed-"))
root, home, bin_dir, log = work / "root", work / "home", work / "bin", work / "calls.log"
dpkg = root / "var/lib/dpkg"
write(dpkg / "status", """Package: winlux-desktop
Status: install ok installed
Priority: required
Essential: yes
Protected: yes
Installed-Size: 50000
Maintainer: Winlux <winlux@localhost>
Version: 0.2.20
Depends: mousepad, libgtk-3-0 | libgtk-4-1

Package: mousepad
Status: install ok installed
Installed-Size: 2000
Maintainer: Xfce Developers <xfce@example.org>
Version: 0.6.1
Depends: libgtk-3-0

Package: libgtk-3-0
Status: install ok installed
Installed-Size: 9000
Maintainer: Ubuntu Developers <ubuntu@example.org>
Version: 3.24

Package: vlc
Status: install ok installed
Installed-Size: 4096
Maintainer: Ubuntu Developers <ubuntu@example.org>
Version: 3.0.21
Description: lecteur multimédia

Package: libreoffice-common
Status: install ok installed
Installed-Size: 100
Maintainer: Debian LibreOffice Maintainers <lo@example.org>
Version: 24.2

Package: ancien
Status: deinstall ok config-files
Installed-Size: 1
Version: 1
""")
for package, files in {"vlc": ["/usr/share/applications/vlc.desktop", "/usr/bin/vlc"],
                       "mousepad": ["/usr/share/applications/org.xfce.mousepad.desktop"],
                       "libreoffice-common": ["/usr/share/applications/libreoffice-startcenter.desktop",
                                              "/usr/share/applications/libreoffice-xsltfilter.desktop"],
                       "exo-utils:amd64": ["/usr/share/applications/xfce4-file-manager.desktop"]}.items():
    write(dpkg / f"info/{package}.list", "\n".join(files) + "\n")
old = time.mktime((2026, 10, 1, 12, 0, 0, 0, 0, -1))
os.utime(dpkg / "info/vlc.list", (old, old))
apps = root / "usr/share/applications"
write(apps / "vlc.desktop", "[Desktop Entry]\nType=Application\nName=VLC media player\nName[de]=VLC Media Player\nIcon=vlc\nExec=vlc %U\n")
write(apps / "org.xfce.mousepad.desktop", "[Desktop Entry]\nType=Application\nName=Mousepad\nIcon=org.xfce.mousepad\nExec=mousepad\n")
write(apps / "libreoffice-startcenter.desktop", "[Desktop Entry]\nType=Application\nName=LibreOffice\nExec=libreoffice\n")
write(apps / "libreoffice-xsltfilter.desktop", "[Desktop Entry]\nType=Application\nName=XSLT\nNoDisplay=true\nExec=x\n")
write(apps / "xfce4-file-manager.desktop", "[Desktop Entry]\nType=Application\nName=File Manager\nName[fr]=Gestionnaire de fichiers\nExec=exo-open --launch FileManager\n")
write(apps / "winlux-explorer.desktop", "[Desktop Entry]\nType=Application\nName=Explorer Winlux\nExec=winlux-shell-app explorer\n")
write(apps / "kde-only.desktop", "[Desktop Entry]\nType=Application\nName=KDE\nOnlyShowIn=KDE;\nExec=x\n")
local = home / ".local/share/applications"
program = home / ".local/share/winlux/drive_c/Program Files/Notepad++"
portable = home / "Téléchargements/putty.exe"
write(program / "notepad++.exe", "MZ" + "x" * 4000)
write(portable, "MZ" + "y" * 1000)
icon_png = home / ".local/share/winlux/icons/npp.png"
write(icon_png, "png")
write(local / "winlux-pe-npp.desktop", f"[Desktop Entry]\nType=Application\nName=Notepad++\nIcon={icon_png}\nExec=x\nX-Winlux-Executable={program / 'notepad++.exe'}\nX-Winlux-Generated-Hash=1\n")
write(local / "winlux-pe-putty.desktop", f"[Desktop Entry]\nType=Application\nName=PuTTY\nIcon=putty\nExec=x\nX-Winlux-Executable={portable}\nX-Winlux-Generated-Hash=1\n")
write(local / "winlux-browser-firefox.desktop", "[Desktop Entry]\nType=Application\nName=Firefox\nExec=x\nX-Winlux-Generated-Hash=1\n")
write(local / "mon-appimage.desktop", "[Desktop Entry]\nType=Application\nName=Mon AppImage\nExec=/home/x/app.AppImage\n")
write(home / ".local/share/flatpak/exports/share/applications/org.gimp.GIMP.desktop", "[Desktop Entry]\nType=Application\nName=GIMP\nIcon=org.gimp.GIMP\nExec=gimp\n")
(home / ".local/share/flatpak/app/org.gimp.GIMP").mkdir(parents=True)
write(bin_dir / "winlux", f"""#!/bin/sh
echo "winlux $*" >> "{log}"
if test "$2" = --programs; then
  printf 'app=HKLM\\\\Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Uninstall\\\\Notepad++\\tNotepad++\\t8.7.9\\tDon Ho\\t4500\\t20261105\\tnsis\\n'
  printf 'app=HKCU\\\\Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Uninstall\\\\Outil\\tOutil maison\\t\\t\\t0\\t\\texe\\n'
  echo done=1
fi
""", 0o755)
write(bin_dir / "flatpak", f"""#!/bin/sh
echo "flatpak $*" >> "{log}"
case "$*" in *list*) printf 'org.gimp.GIMP\\tGNU Image Manipulation Program\\t2.10.38\\tflathub\\tuser\\t312.5 MB\\n'
  printf 'org.example.Shared\\tPartagée\\t1.0\\tflathub\\tsystem\\t1.2 GB\\n' ;; esac
""", 0o755)
write(bin_dir / "apt-get", f"""#!/bin/sh
echo "apt-get $*" >> "{log}"
case "$*" in
  "-s remove vlc") echo "Remv vlc [3.0.21]"; echo "Remv vlc-plugin-base [3.0.21]" ;;
  "-s remove libreoffice-common") echo "Remv libreoffice-common [24.2]"; echo "Remv winlux-desktop [0.2.20]" ;;
esac
""", 0o755)
write(bin_dir / "winlux-root", f"#!/bin/sh\necho \"winlux-root $*\" >> \"{log}\"\n", 0o755)
write(work / "root.sock", "")
os.environ.update({"WINLUX_INSTALLED_ROOT": str(root), "HOME": str(home), "XDG_DATA_HOME": str(home / ".local/share"),
                   "XDG_DATA_DIRS": "/usr/share", "PATH": f"{bin_dir}:{os.environ.get('PATH', '/usr/bin:/bin')}",
                   "WINLUX_ROOT_SOCKET": str(work / "root.sock"), "WINLUX_LANG": "fr",
                   "WINLUX_APPLICATIONS": str(HERE / "winlux-applications.py")})

module = runpy.run_path(str(HERE / "winlux-installed-apps.py"), run_name="installed_apps_test")
module["os"].geteuid = lambda: 1000  # bureau sous le compte de l'utilisateur, jamais root
collect = module["collect"]
found = {app["id"]: app for app in collect()}
names = sorted(app["name"] for app in found.values())
print("Applications trouvées :", names)

vlc = found.get("apt:vlc")
check(vlc and vlc["name"] == "VLC media player" and vlc["size"] == 4096 * 1024 and vlc["publisher"] == "Ubuntu Developers"
      and vlc["removable"] and not vlc["system"] and vlc["version"] == "3.0.21", f"VLC (paquet APT) : {vlc}")
check(vlc and time.localtime(vlc["date"])[:3] == (2026, 10, 1), "date d'installation = liste dpkg du paquet")
check(found.get("apt:mousepad", {}).get("system") and not found["apt:mousepad"]["removable"],
      "dépendance de winlux-desktop : composant du système, non désinstallable")
office = found.get("apt:libreoffice-common")
check(office and office["name"] == "LibreOffice" and office["also"] == [], "lanceur caché (NoDisplay) non compté")
check(not any("Gestionnaire" in name or name == "File Manager" for name in names), "#721 : « Gestionnaire de fichiers » absent")
check(not any(name == "KDE" for name in names), "OnlyShowIn=KDE ignoré")
check(not any(name == "Firefox" for name in names), "lanceur généré (winlux-browser-*) non dupliqué")
explorer = found.get(f"desktop:{apps / 'winlux-explorer.desktop'}")
check(explorer and explorer["system"] and not explorer["removable"], "Explorateur : composant de Winlux")
mine = found.get(f"desktop:{local / 'mon-appimage.desktop'}")
check(mine and not mine["system"] and not mine["removable"], "lanceur ajouté à la main : listé, sans désinstallation")
gimp = found.get("flatpak:user:org.gimp.GIMP")
check(gimp and gimp["name"] == "GIMP" and gimp["size"] == 312500000 and gimp["removable"], f"Flatpak utilisateur : {gimp}")
shared = found.get("flatpak:system:org.example.Shared")
check(shared and not shared["removable"] and shared["size"] == 1200000000, "Flatpak système : pas de désinstallation sans root")
npp = next((app for key, app in found.items() if key.startswith("windows:") and app["name"] == "Notepad++"), None)
check(npp and npp["size"] == 4500 * 1024 and npp["publisher"] == "Don Ho" and npp["removable"]
      and time.localtime(npp["date"])[:3] == (2026, 11, 5), f"programme Windows inscrit : {npp}")
check(npp and npp["icon"] == str(icon_png), "icône du programme Windows reprise de son lanceur")
check("exe:" + str(program / "notepad++.exe") not in found, "lanceur du programme inscrit non compté deux fois")
putty = found.get("exe:" + str(portable))
check(putty and putty["kind"] == "portable" and not putty["removable"] and putty["size"] == 1002, f"programme portable : {putty}")
check(any(app["name"] == "Outil maison" for app in found.values()), "programme Windows sans taille ni date")

# ── Désinstallation : le bon outil, root par le relais ──
command = module["uninstall_command"]
check(command("apt:vlc") == ["winlux-root", "exec", "apt-get", "-y", "remove", "vlc"], f"APT par le relais root : {command('apt:vlc')}")
check(command("snap:spotify") == ["winlux-root", "exec", "snap", "remove", "spotify"], "Snap par le relais root")
check(command("flatpak:user:org.gimp.GIMP") == ["flatpak", "uninstall", "--user", "-y", "--noninteractive", "org.gimp.GIMP"], "Flatpak --user")
check(command("flatpak:system:org.example.Shared") is None, "Flatpak système refusé sans root")
check(command(npp["id"])[:3] == ["winlux", "--setup", "--remove"] and command(npp["id"])[3].endswith("Notepad++"), "Windows : winlux --setup --remove CLÉ")
check(command("apt:vlc;rm -rf /") is None and command("apt:-y") is None, "nom de paquet refusé")
check(module["removal_check"](vlc) == (True, ["vlc-plugin-base"]), f"simulation APT : {module['removal_check'](vlc)}")
office["removable"] = True
check(module["removal_check"](office) == (False, []), "retrait qui emporterait winlux-desktop refusé")
log.write_text("")
check(module["main"](["x", "uninstall", "apt:vlc"]) == 0 and "winlux-root exec apt-get -y remove vlc" in log.read_text(), "CLI uninstall")
check(module["main"](["x", "uninstall", "apt:mousepad"]) == 1, "CLI : composant du système refusé")

# ── Formats et langues ──
check(module["parse_size"]("312.5 MB") == 312500000 and module["parse_size"]("512 kB") == 512000 and module["parse_size"]("?") == 0, "tailles Flatpak")
check(module["human_size"](4096 * 1024) == "4,0 Mo", module["human_size"](4096 * 1024))
for text, row in module["_TR"].items():
    check(len(row) == 3 and all(row), f"traduction incomplète : {text}")
source = (HERE / "winlux-installed-apps.py").read_text()
for text in set(re.findall(r'_t\("([^"]+)"', source)):
    check(text in module["_TR"], f"texte sans traduction : {text}")

# ── Paramètres : page 39 branchée, menu Démarrer sans doublon ──
settings = (HERE / "winlux-settings.py").read_text()
pages_source = (HERE / "winlux-settings-pages.py").read_text()
check('_t("Sauvegarde"), _t("Applications installées")]' in settings, "page 39 = Applications installées")
check('(_t("Applications"), [39, 1, 10, 11, 15, 30])' in settings, "première page de la catégorie Applications")
check('"appsfeatures": 39' in settings, "ms-settings:appsfeatures")
check("PAGE_INSTALLED = 39" in pages_source and 'PAGE_INSTALLED: "page_installed"' in pages_source, "page branchée")
start = (HERE / "winlux-start-menu.py").read_text()
check(re.search(r'SESSION_SERVICES = \{[^}]*"xfce4-file-manager\.desktop"', start), "#721 : menu Démarrer sans xfce4-file-manager")
check('rm -f "$stage/usr/share/applications/xfce4-file-manager.desktop"' in (HERE / "stage-desktop-components.sh").read_text(), "#721 : lanceur retiré de l'image")
check("'winlux-installed-apps.py:winlux-installed-apps'" in (HERE / "stage-desktop-components.sh").read_text(), "installé dans /usr/bin")

# ── La page elle-même (écran requis) ──
if os.environ.get("DISPLAY"):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
    window = Gtk.Window()
    window.set_default_size(700, 700)
    scroll = Gtk.ScrolledWindow()
    window.add(scroll)
    panel = module["InstalledAppsPanel"](window)
    scroll.add(panel)
    window.show_all()
    deadline = time.time() + 20
    while not panel.rows and time.time() < deadline:
        Gtk.main_iteration_do(False)
    for _ in range(50): Gtk.main_iteration_do(False)
    check(len(panel.rows) == len(found), f"une ligne par application ({len(panel.rows)}/{len(found)})")

    def shown():
        return [row.app["name"] for row in panel.list.get_children() if row.get_child_visible() and panel.matches(row)]

    def ordered():
        rows = [row for row in panel.list.get_children() if panel.matches(row)]
        return [row.app["name"] for row in sorted(rows, key=lambda row: row.get_index())]

    def settle(seconds=0.4):
        end = time.time() + seconds  # la recherche attend 150 ms de calme (GtkSearchEntry)
        while time.time() < end:
            Gtk.main_iteration_do(False)
            time.sleep(0.01)

    panel.search.set_text("vlc")
    settle()
    check(shown() == ["VLC media player"], f"recherche : {shown()}")
    check("1 application trouvée" in panel.count.get_text(), panel.count.get_text())
    panel.search.set_text("")
    settle()
    panel.filter.set_active_id("windows")
    check(sorted(shown()) == ["Notepad++", "Outil maison", "PuTTY"], f"filtre Windows : {shown()}")
    panel.filter.set_active_id("all")
    panel.sort.set_active_id("size")
    settle()
    check(ordered()[0] == "Partagée", f"tri par taille : {ordered()[:3]}")
    panel.sort.set_active_id("name")
    settle()
    check(ordered() == sorted(ordered(), key=str.casefold), "tri par nom")
    buttons = {row.app["id"]: row.uninstall_button for row in panel.list.get_children()}
    check(buttons["apt:vlc"].get_sensitive() and not buttons["apt:mousepad"].get_sensitive()
          and buttons["apt:mousepad"].get_tooltip_text(), "Désinstaller grisé pour un composant du système, avec la raison")
    width = panel.get_preferred_width()[0]
    check(width <= 640, f"page trop large pour 1366x768 : {width} px")
    window.destroy()
else:
    print("Pas d'écran : la partie GTK est sautée.")

if problems:
    print(f"\n{len(problems)} problème(s).")
    sys.exit(1)
print("Applications installées : OK")
