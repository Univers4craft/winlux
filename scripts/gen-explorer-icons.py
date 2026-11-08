#!/usr/bin/env python3
"""Génère les icônes intégrées à l'Explorateur (#589) depuis le jeu maison
« Prisme » (assets/icons/explorateur/), le même que celui de la session
native : un seul Explorateur, un seul style.

Fichiers source : `<nom>-<taille>.png` (thème sombre, par défaut) et
`<nom>-clair-<taille>.png` (thème clair), PNG RGBA de 16 à 48 px. Les glyphes
de barre d'outils sont monochromes : seul leur alpha est gardé, la teinte suit
le thème à l'affichage.

Sortie (versionnée) :
- testexe/src/shell/explorer_icons.bin : pixels bruts concaténés (BGRA
  prémultiplié pour les icônes en couleur, alpha seul pour les glyphes).
- testexe/src/shell/explorer_icons_data.rs : table des images.

Usage : python3 scripts/gen-explorer-icons.py  (Pillow)
"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "assets/icons/explorateur"
OUT_BIN = ROOT / "testexe/src/shell/explorer_icons.bin"
OUT_RS = ROOT / "testexe/src/shell/explorer_icons_data.rs"

SIZES = [16, 20, 24, 32, 48]

# (variante Rust, nom du fichier)
COLOR = [
    ("Folder", "dossier"),
    ("FolderDocuments", "documents"),
    ("FolderDownload", "telechargements"),
    ("FolderPictures", "images"),
    ("FolderMusic", "musique"),
    ("FolderVideo", "videos"),
    ("Desktop", "bureau"),
    ("Home", "accueil"),
    ("Trash", "corbeille-vide"),
    ("TrashFull", "corbeille-pleine"),
    ("Drive", "disque-c"),
    ("LinuxDrive", "disque-z"),
    ("Computer", "ce-pc"),
    ("Removable", "lecteur-amovible"),
    ("Network", "lecteur-reseau"),
    ("Text", "type-texte"),
    ("Pdf", "type-pdf"),
    ("Archive", "type-archive"),
    ("Audio", "type-audio"),
    ("Video", "type-video"),
    ("Image", "type-image"),
    ("Exe", "type-exe"),
    ("Code", "type-code"),
    ("Script", "type-script"),
    ("Web", "type-web"),
    ("Document", "type-document"),
    ("Spreadsheet", "type-tableur"),
    ("Presentation", "type-presentation"),
    ("Font", "type-police"),
    ("Globe", "reseau"),
    ("DiskImage", "type-iso"),
    ("File", "fichier-inconnu"),
]
GLYPHS = [
    ("Cut", "couper"),
    ("Copy", "copier"),
    ("Paste", "coller"),
    ("Delete", "supprimer"),
    ("Rename", "renommer"),
    ("Share", "partager"),
    ("Sort", "trier"),
    ("View", "afficher"),
    ("New", "nouveau"),
    ("Terminal", "terminal"),
    ("Search", "rechercher"),
    ("Details", "details"),
    ("More", "options"),
    ("ChevronDown", "chevron-bas"),
    ("Split", "double-volet"),
]


def pixels(im: Image.Image):
    im = im.convert("RGBA")
    data = im.get_flattened_data() if hasattr(im, "get_flattened_data") else im.getdata()
    return list(data)


def premultiplied_bgra(im: Image.Image) -> bytes:
    out = bytearray()
    for r, g, b, a in pixels(im):
        out += bytes(((b * a + 127) // 255, (g * a + 127) // 255, (r * a + 127) // 255, a))
    return bytes(out)


def alpha(im: Image.Image) -> bytes:
    return bytes(a for *_, a in pixels(im))


def load(name: str, size: int, light: bool) -> Image.Image:
    path = SRC / f"{name}-{'clair-' if light else ''}{size}.png"
    im = Image.open(path)
    if im.size != (size, size):
        raise SystemExit(f"{path.name} : {im.size} au lieu de {size}x{size}")
    return im


def main() -> None:
    blob = bytearray()
    rows = []  # (variante, taille, décalage, alpha seul, variante claire)
    for variant, name in COLOR:
        for light in (False, True):
            for size in SIZES:
                rows.append((variant, size, len(blob), False, light))
                blob += premultiplied_bgra(load(name, size, light))
    for variant, name in GLYPHS:
        for size in SIZES:
            rows.append((variant, size, len(blob), True, False))
            blob += alpha(load(name, size, False))
    OUT_BIN.write_bytes(bytes(blob))
    variants = [v for v, _ in COLOR + GLYPHS]
    lines = [
        "// Généré par scripts/gen-explorer-icons.py : ne pas modifier à la main.",
        "// Icônes maison « Prisme » (assets/icons/explorateur), comme la session native.",
        "",
        "#[allow(dead_code)]",
        "#[derive(Clone, Copy, PartialEq, Eq, Debug)]",
        "pub(super) enum Icon {",
        *[f"    {v}," for v in variants],
        "}",
        "",
        f"pub(super) const ICON_BYTES: usize = {len(blob)};",
        "",
        "/// (icône, taille en pixels, décalage dans explorer_icons.bin, alpha seul,",
        "/// variante du thème clair)",
        f"pub(super) static ICON_TABLE: [(Icon, u16, u32, bool, bool); {len(rows)}] = [",
        *[f"    (Icon::{v}, {s}, {o}, {str(m).lower()}, {str(l).lower()})," for v, s, o, m, l in rows],
        "];",
        "",
    ]
    OUT_RS.write_text("\n".join(lines), encoding="utf-8")
    print(f"{len(rows)} images, {len(blob)} octets")


if __name__ == "__main__":
    main()
