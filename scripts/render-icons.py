#!/usr/bin/python3
"""Thème d'icônes « Winlux » (freedesktop), dessiné ici en SVG.

Ce script ne dessine plus que les icônes d'applications (scalable/apps) :
pictogrammes blancs sur tuile de couleur, couleurs de la marque Prisme
(violet → cyan). Emplacements, périphériques, types de fichiers, l'index.theme
et le thème Winlux-Sombre viennent de scripts/generer-icones.py. Relancer :

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
    "phone": ("cyan", '<rect x="23" y="15" width="18" height="34" rx="4"/><path d="M30 43h4"/>', "Téléphone connecté"),
    "applications-games": ("prisme", '<path d="M22 24h20a8 8 0 0 1 7.6 5.5l2.6 8.6A4.5 4.5 0 0 1 44.6 42L40 37H24l-4.6 5a4.5 4.5 0 0 1-7.6-3.9l2.6-8.6A8 8 0 0 1 22 24z"/><path d="M24 28v6M21 31h6"/><circle cx="40" cy="29" r="1.2" fill="#fff"/><circle cx="43" cy="33" r="1.2" fill="#fff"/>', "Jeux"),
    "system-help": ("violet", '<circle cx="32" cy="32" r="14"/><path d="M27 28a5 5 0 1 1 7 4.5c-1.5.8-2 1.7-2 3.5M32 41v.5"/>', "Aide"),
}

def logo():
    return HEAD + PRISME + "<title>Winlux</title></svg>\n"


def main():
    out = {}
    for name, (color, glyph, title) in APPS.items():
        out[f"scalable/apps/{name}.svg"] = tile(color, glyph, title)
    out["scalable/apps/winlux.svg"] = logo()
    for rel, svg in out.items():
        path = THEME / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(svg)
    print(f"{len(out)} icônes écrites dans {THEME}")


if __name__ == "__main__":
    main()
