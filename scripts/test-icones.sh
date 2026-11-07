#!/bin/sh
# Vérifie le jeu d'icônes Winlux « Prisme » (clair et sombre).
# Usage : scripts/test-icones.sh [racine]   (racine = dossier contenant Winlux/ et Winlux-Sombre/,
#         par défaut /usr/share/icons). À appeler depuis scripts/test-all.sh.
set -eu
racine=${1:-/usr/share/icons}
echec=0
ko() { echo "ÉCHEC : $*"; echec=1; }

noms_places="folder user-desktop folder-documents folder-download folder-pictures folder-music folder-videos
folder-remote user-home user-trash user-trash-full document-open-recent network-workgroup start-here"
noms_devices="computer drive-harddisk drive-harddisk-system drive-removable-media drive-optical media-optical"
noms_mime="text-x-generic text-plain application-x-shellscript text-rust text-html application-json application-pdf
image-png image-jpeg audio-mpeg video-mp4 x-office-document x-office-spreadsheet x-office-presentation font-ttf
application-vnd.debian.binary-package application-zip application-x-executable
application-vnd.microsoft.portable-executable application-x-msdownload application-x-msi application-vnd.efi.iso"

for theme in Winlux Winlux-Sombre; do
    d="$racine/$theme"
    test -f "$d/index.theme" || { ko "$theme : index.theme absent"; continue; }
    for ctx_noms in "places:$noms_places" "devices:$noms_devices" "mimetypes:$noms_mime"; do
        ctx=${ctx_noms%%:*}; noms=${ctx_noms#*:}
        for n in $noms; do
            test -e "$d/scalable/$ctx/$n.svg" || ko "$theme : scalable/$ctx/$n.svg manquant"
            test -e "$d/16/$ctx/$n.svg" || ko "$theme : 16/$ctx/$n.svg (dessin petit) manquant"
            for t in 16 20 24 32 48 64 96 128 256; do
                test -e "$d/${t}x${t}/$ctx/$n.png" || ko "$theme : ${t}x${t}/$ctx/$n.png manquant"
            done
        done
    done
    # Aucun lien cassé ni boucle (un alias identique au nom détruisait l'icône).
    casses=$(find "$d" -xtype l | head -5)
    test -z "$casses" || ko "$theme : liens cassés : $casses"
    # Pas de marque tierce dans les SVG.
    if grep -rliE 'microsoft|windows logo|ubuntu|canonical' "$d" --include='*.svg' >/dev/null 2>&1; then
        ko "$theme : un SVG mentionne une marque tierce"
    fi
done
grep -q '^Inherits=Winlux' "$racine/Winlux-Sombre/index.theme" 2>/dev/null || ko "Winlux-Sombre doit hériter de Winlux"

# GTK doit résoudre les noms du thème, et non ceux d'Adwaita.
if command -v python3 >/dev/null && python3 -c 'import gi' 2>/dev/null; then
    WINLUX_ICONES="$racine" python3 - <<'EOF' || echec=1
import os, gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
racine = os.environ["WINLUX_ICONES"]
ok = True
for theme in ("Winlux", "Winlux-Sombre"):
    t = Gtk.IconTheme(); t.set_search_path([racine, "/usr/share/icons"]); t.set_custom_theme(theme)
    for n in ("folder-documents", "user-home", "document-open-recent", "audio-mpeg", "application-vnd.microsoft.portable-executable", "drive-harddisk-system"):
        for s in (16, 24, 48, 256):
            i = t.lookup_icon(n, s, 0)
            f = i.get_filename() if i else None
            if not f or "/Adwaita/" in f:
                print(f"ÉCHEC : {theme} {n} {s}px -> {f}"); ok = False
raise SystemExit(0 if ok else 1)
EOF
fi

test $echec -eq 0 && echo "icônes Prisme : OK"
exit $echec
