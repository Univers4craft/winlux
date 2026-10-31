#!/usr/bin/python3
"""Thème d'icônes « Winlux » (freedesktop), dessiné ici en SVG.

Un seul langage visuel : coins arrondis, dégradés doux, pictogrammes blancs
sur tuile de couleur pour les applications, dossiers ambrés avec emblème
pour les emplacements. Couleurs de la marque Prisme (violet → cyan).
Les icônes absentes viennent d'Adwaita (Inherits). Relancer :

    python3 scripts/render-icons.py
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
THEME = ROOT / "integration" / "native" / "icons" / "Winlux"

HEAD = '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">'
SHADOW = '<rect x="7" y="10" width="50" height="50" rx="12" fill="#000" opacity=".16"/>'

# Tuiles d'application : (haut, bas) du dégradé.
TILES = {
    "violet": ("#9C7BFF", "#5B2FE0"),
    "cyan": ("#5FD8FF", "#1478D4"),
    "prisme": ("#8F6BFF", "#1E90E8"),
    "vert": ("#4FD69C", "#0E8F62"),
    "orange": ("#FFB45C", "#E8641E"),
    "rouge": ("#FF7A85", "#D12F4A"),
    "ardoise": ("#6E7891", "#343B4F"),
    "nuit": ("#3A3360", "#14112E"),
}


def tile(color, glyph, name):
    top, bottom = TILES[color]
    return (
        HEAD
        + f'<defs><linearGradient id="t" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{top}"/>'
        f'<stop offset="1" stop-color="{bottom}"/></linearGradient>'
        '<linearGradient id="h" x1="0" y1="0" x2="0" y2="1"><stop stop-color="#fff" stop-opacity=".28"/>'
        '<stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>'
        + SHADOW
        + '<rect x="6" y="6" width="52" height="52" rx="12" fill="url(#t)"/>'
        '<rect x="6" y="6" width="52" height="52" rx="12" fill="url(#h)"/>'
        f'<g fill="none" stroke="#fff" stroke-width="3.6" stroke-linecap="round" stroke-linejoin="round">{glyph}</g>'
        f"<title>{name}</title></svg>\n"
    )


PRISME = (
    '<polygon points="32,6 6,32 32,26.9" fill="#B69CFF"/><polygon points="6,32 32,26.9 32,58" fill="#5E35E6"/>'
    '<polygon points="32,6 58,32 32,26.9" fill="#7DE8FF"/><polygon points="32,26.9 58,32 32,58" fill="#1A8AE6"/>'
)

APPS = {
    # nom freedesktop : (couleur, pictogramme, titre)
    "preferences-desktop-theme": ("violet", '<circle cx="32" cy="32" r="14"/><path d="M32 18v28" /><path d="M32 18a14 14 0 0 1 0 28" fill="#fff"/>', "Personnalisation"),
    "system-software-install": ("cyan", '<path d="M32 17v20M24 30l8 8 8-8"/><path d="M18 42v5h28v-5"/>', "Applications"),
    "web-browser": ("cyan", '<circle cx="32" cy="32" r="14"/><path d="M18 32h28M32 18c-6 8-6 20 0 28M32 18c6 8 6 20 0 28"/>', "Navigateur"),
    "system-file-manager": ("orange", '<path d="M17 22h11l3 4h16v18H17z"/>', "Fichiers"),
    "security-high": ("vert", '<path d="M32 16l13 5v10c0 9-6 15-13 18-7-3-13-9-13-18V21z"/><path d="M26 32l4 4 8-8"/>', "Pare-feu"),
    "preferences-system-notifications": ("violet", '<path d="M22 40h20l-3-5v-7a7 7 0 0 0-14 0v7z"/><path d="M29 44a3 3 0 0 0 6 0"/>', "Notifications"),
    "preferences-desktop-screensaver": ("nuit", '<path d="M18 26h10v12h12V24"/><circle cx="18" cy="26" r="2" fill="#fff"/><circle cx="40" cy="24" r="2" fill="#fff"/>', "Écran de veille"),
    "preferences-system": ("ardoise", '<circle cx="32" cy="32" r="6"/><path d="M32 16v5M32 43v5M16 32h5M43 32h5M20.7 20.7l3.5 3.5M39.8 39.8l3.5 3.5M20.7 43.3l3.5-3.5M39.8 24.2l3.5-3.5"/>', "Paramètres"),
    "utilities-system-monitor": ("vert", '<path d="M16 34h8l4-10 6 18 4-8h10"/>', "Gestionnaire de tâches"),
    "x-office-calendar": ("rouge", '<rect x="18" y="20" width="28" height="26" rx="4"/><path d="M18 28h28M25 16v7M39 16v7"/>', "Planificateur"),
    "utilities-terminal": ("nuit", '<path d="M20 24l8 8-8 8M32 42h12"/>', "Terminal"),
    "multimedia-video-player": ("orange", '<path d="M27 22l15 10-15 10z" fill="#fff"/>', "Lecteur multimédia"),
    "network-workgroup": ("cyan", '<rect x="15" y="22" width="14" height="11" rx="2"/><rect x="35" y="31" width="14" height="11" rx="2"/><path d="M29 28h4v8h2"/>', "Transfert de PC"),
    "accessories-text-editor": ("cyan", '<path d="M22 18h14l7 7v21H22z"/><path d="M27 32h11M27 38h8"/>', "Bloc-notes"),
    "accessories-calculator": ("ardoise", '<rect x="21" y="17" width="22" height="30" rx="3"/><path d="M26 24h12M26 33h1M32 33h1M38 33h1M26 40h1M32 40h1M38 40h1"/>', "Calculatrice"),
    "system-help": ("violet", '<circle cx="32" cy="32" r="14"/><path d="M27 28a5 5 0 1 1 7 4.5c-1.5.8-2 1.7-2 3.5M32 41v.5"/>', "Aide"),
}

FOLDER_BACK = "#D99A1E"
FOLDER_FRONT = ("#FFD978", "#F2AE34")


def folder(emblem, name):
    return (
        HEAD
        + f'<defs><linearGradient id="f" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{FOLDER_FRONT[0]}"/>'
        f'<stop offset="1" stop-color="{FOLDER_FRONT[1]}"/></linearGradient></defs>'
        '<path d="M5 15a5 5 0 0 1 5-5h14l6 7h24a5 5 0 0 1 5 5v29a5 5 0 0 1-5 5H10a5 5 0 0 1-5-5z" '
        f'fill="{FOLDER_BACK}"/>'
        '<rect x="8" y="20" width="48" height="30" rx="3" fill="#F6F1FB"/>'
        '<path d="M4 26a3 3 0 0 1 3-3h50a3 3 0 0 1 3 3l-3 27a4 4 0 0 1-4 3.6H11a4 4 0 0 1-4-3.6z" fill="url(#f)"/>'
        + (f'<g fill="none" stroke="#9A5B0C" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" opacity=".8">{emblem}</g>' if emblem else "")
        + f"<title>{name}</title></svg>\n"
    )


PLACES = {
    "folder": ("", "Dossier"),
    "folder-documents": ('<path d="M26 32h12M26 38h12M26 44h8"/>', "Documents"),
    "folder-download": ('<path d="M32 30v14M26 39l6 6 6-6"/>', "Téléchargements"),
    "folder-pictures": ('<rect x="23" y="31" width="18" height="14" rx="2"/><path d="M24 44l6-6 4 4 3-3 4 4"/>', "Images"),
    "folder-music": ('<path d="M29 44V32l10-2v12"/><circle cx="27" cy="44" r="2.5"/><circle cx="37" cy="42" r="2.5"/>', "Musique"),
    "folder-videos": ('<path d="M28 32l11 6-11 6z"/>', "Vidéos"),
    "user-desktop": ('<rect x="22" y="31" width="20" height="12" rx="2"/><path d="M28 47h8"/>', "Bureau"),
    "user-home": ('<path d="M23 38l9-8 9 8M26 36v9h12v-9"/>', "Dossier personnel"),
    "folder-remote": ('<circle cx="32" cy="39" r="7"/><path d="M25 39h14M32 32c-3 4-3 10 0 14M32 32c3 4 3 10 0 14"/>', "Lecteur réseau"),
}


def drive(kind, name):
    body = {
        "drive-harddisk": '<rect x="8" y="22" width="48" height="24" rx="5" fill="url(#d)"/>'
        '<rect x="8" y="38" width="48" height="8" rx="4" fill="#3C4458" opacity=".25"/>'
        '<circle cx="47" cy="38" r="2.5" fill="#4FD69C"/>',
        "drive-removable-media": '<rect x="18" y="12" width="28" height="40" rx="5" fill="url(#d)"/>'
        '<rect x="25" y="6" width="14" height="9" rx="2" fill="#9AA3B5"/>'
        '<path d="M27 9h2M35 9h2" stroke="#59627A" stroke-width="2"/>'
        '<circle cx="32" cy="40" r="3" fill="#7C4DFF"/>',
        "computer": '<rect x="7" y="12" width="50" height="32" rx="4" fill="#2C2F42"/>'
        '<rect x="10" y="15" width="44" height="26" rx="2" fill="url(#s)"/>'
        '<path d="M26 44h12l2 8H24z" fill="#9AA3B5"/><rect x="18" y="51" width="28" height="4" rx="2" fill="#C7CDD9"/>',
        "user-trash": '<path d="M18 20h28l-3 34a3 3 0 0 1-3 3H24a3 3 0 0 1-3-3z" fill="url(#d)"/>'
        '<rect x="14" y="14" width="36" height="6" rx="3" fill="#9AA3B5"/>'
        '<path d="M27 27v22M37 27v22" stroke="#7B8499" stroke-width="2.5" stroke-linecap="round"/>',
        "user-trash-full": '<path d="M18 20h28l-3 34a3 3 0 0 1-3 3H24a3 3 0 0 1-3-3z" fill="url(#d)"/>'
        '<path d="M20 18l8-8 6 6 6-5 6 7" fill="#F6F1FB" stroke="#B9C0CF" stroke-width="1.5"/>'
        '<rect x="14" y="14" width="36" height="6" rx="3" fill="#9AA3B5"/>'
        '<path d="M27 27v22M37 27v22" stroke="#7B8499" stroke-width="2.5" stroke-linecap="round"/>',
    }[kind]
    return (
        HEAD
        + '<defs><linearGradient id="d" x1="0" y1="0" x2="0" y2="1"><stop stop-color="#E9ECF3"/>'
        '<stop offset="1" stop-color="#B9C0CF"/></linearGradient>'
        '<linearGradient id="s" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#2A1C66"/>'
        '<stop offset=".55" stop-color="#5B2FE0"/><stop offset="1" stop-color="#1E90E8"/></linearGradient></defs>'
        + body
        + f"<title>{name}</title></svg>\n"
    )


def executable():
    """Programme Windows (.exe) : fenêtre claire portant le logo Prisme."""
    return (
        HEAD
        + SHADOW
        + '<rect x="6" y="8" width="52" height="48" rx="7" fill="#F4F2FA"/>'
        '<path d="M6 15a7 7 0 0 1 7-7h38a7 7 0 0 1 7 7v3H6z" fill="#5B2FE0"/>'
        '<circle cx="47" cy="13" r="1.6" fill="#fff"/><circle cx="52" cy="13" r="1.6" fill="#fff"/>'
        '<g transform="translate(19 23) scale(.40625)">' + PRISME + "</g>"
        "<title>Programme Windows</title></svg>\n"
    )


def logo():
    return HEAD + PRISME + "<title>Winlux</title></svg>\n"


INDEX = """[Icon Theme]
Name=Winlux
Comment=Icônes Winlux (logo Prisme), complétées par Adwaita
Inherits=Adwaita,hicolor
Directories=scalable/apps,scalable/places,scalable/devices,scalable/mimetypes

[scalable/apps]
Size=64
MinSize=16
MaxSize=512
Type=Scalable
Context=Applications

[scalable/places]
Size=64
MinSize=16
MaxSize=512
Type=Scalable
Context=Places

[scalable/devices]
Size=64
MinSize=16
MaxSize=512
Type=Scalable
Context=Devices

[scalable/mimetypes]
Size=64
MinSize=16
MaxSize=512
Type=Scalable
Context=MimeTypes
"""


def main():
    out = {}
    for name, (color, glyph, title) in APPS.items():
        out[f"scalable/apps/{name}.svg"] = tile(color, glyph, title)
    out["scalable/apps/winlux.svg"] = logo()
    out["scalable/places/start-here.svg"] = logo()
    for name, (emblem, title) in PLACES.items():
        out[f"scalable/places/{name}.svg"] = folder(emblem, title)
    for name in ("drive-harddisk", "drive-removable-media", "computer"):
        out[f"scalable/devices/{name}.svg"] = drive(name, name)
    for name in ("user-trash", "user-trash-full"):
        out[f"scalable/places/{name}.svg"] = drive(name, name)
    out["scalable/mimetypes/application-x-ms-dos-executable.svg"] = executable()
    out["scalable/mimetypes/application-x-msdownload.svg"] = executable()
    for rel, svg in out.items():
        path = THEME / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(svg)
    (THEME / "index.theme").write_text(INDEX)
    print(f"{len(out)} icônes écrites dans {THEME}")


if __name__ == "__main__":
    main()
