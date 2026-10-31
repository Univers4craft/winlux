#!/usr/bin/python3
"""Compare deux séries de captures d'écran Winlux (PNG de même nom).

    compare-captures.py AVANT APRES SORTIE [--seuil 2.0] [--tolerance 24]

AVANT est la référence (version précédente), APRES la nouvelle série. Pour
chaque écran, on compte la part de pixels dont une composante a bougé de plus
de --tolerance (sur 255) : l'horloge ou un anticrénelage différent restent
sous le seuil, un panneau déplacé ou une page vide le dépassent.

SORTIE reçoit rapport.md (lisible, prêt pour le résumé GitHub), rapport.json
et, pour chaque écran modifié, NOM-diff.png : la nouvelle image assombrie, les
pixels changés en rouge. Les écrans au-dessus du --seuil (en %) sont signalés
par une annotation « ::warning » quand GITHUB_ACTIONS est défini.

Le code de sortie reste 0 : une modification visuelle peut être voulue, c'est
une alerte à regarder, pas un échec. Seule une erreur de lecture renvoie 2.
"""
import argparse
import json
import os
from pathlib import Path
import sys

import gi
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf, GLib  # noqa: E402


def load(path):
    pixbuf = GdkPixbuf.Pixbuf.new_from_file(str(path))
    if pixbuf.get_has_alpha():
        pixbuf = pixbuf.composite_color_simple(pixbuf.get_width(), pixbuf.get_height(),
                                               GdkPixbuf.InterpType.NEAREST, 255, 8, 0, 0)
    return pixbuf


def rows(pixbuf):
    data = pixbuf.get_pixels()
    stride, channels = pixbuf.get_rowstride(), pixbuf.get_n_channels()
    width = pixbuf.get_width() * channels
    return [data[y * stride:y * stride + width] for y in range(pixbuf.get_height())], channels


def compare(before, after, tolerance):
    """(part de pixels changés en %, image de différence ou None)."""
    if (before.get_width(), before.get_height()) != (after.get_width(), after.get_height()):
        return 100.0, None
    old_rows, old_channels = rows(before)
    new_rows, new_channels = rows(after)
    width, height = after.get_width(), after.get_height()
    changed = 0
    diff = bytearray()
    for old, new in zip(old_rows, new_rows):
        line = bytearray(new[x * new_channels + c] // 3 for x in range(width) for c in range(3)) \
            if old != new else None
        if line is not None:
            for x in range(width):
                a, b = x * old_channels, x * new_channels
                if (abs(old[a] - new[b]) > tolerance or abs(old[a + 1] - new[b + 1]) > tolerance
                        or abs(old[a + 2] - new[b + 2]) > tolerance):
                    changed += 1
                    line[x * 3:x * 3 + 3] = b"\xff\x20\x20"
            diff += line
        else:
            diff += bytes(new[x * new_channels + c] // 3 for x in range(width) for c in range(3))
    if not changed:
        return 0.0, None
    image = GdkPixbuf.Pixbuf.new_from_bytes(GLib.Bytes.new(bytes(diff)), GdkPixbuf.Colorspace.RGB,
                                           False, 8, width, height, width * 3)
    return changed * 100.0 / (width * height), image


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("before")
    parser.add_argument("after")
    parser.add_argument("output")
    parser.add_argument("--seuil", type=float, default=2.0)
    parser.add_argument("--tolerance", type=int, default=24)
    args = parser.parse_args()
    before, after, output = Path(args.before), Path(args.after), Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    old = {p.stem: p for p in before.glob("*.png")} if before.is_dir() else {}
    new = {p.stem: p for p in after.glob("*.png")}
    github = bool(os.environ.get("GITHUB_ACTIONS"))
    results = {"seuil": args.seuil, "ecrans": {}, "nouveaux": sorted(set(new) - set(old)),
               "disparus": sorted(set(old) - set(new))}
    try:
        for name in sorted(set(new) & set(old)):
            share, image = compare(load(old[name]), load(new[name]), args.tolerance)
            results["ecrans"][name] = round(share, 3)
            if image is not None:
                image.savev(str(output / f"{name}-diff.png"), "png", [], [])
            if share >= args.seuil and github:
                print(f"::warning title=Capture modifiée::{name} : {share:.1f} % des pixels ont changé "
                      f"(voir l'artefact captures-ui-diff)")
    except GLib.Error as error:
        print(f"Lecture d'image impossible : {error.message}", file=sys.stderr)
        return 2
    alerts = {k: v for k, v in results["ecrans"].items() if v >= args.seuil}
    small = {k: v for k, v in results["ecrans"].items() if 0 < v < args.seuil}
    lines = ["## Captures d'écran", ""]
    if not old:
        lines.append(f"Pas de référence précédente : {len(new)} capture(s) servent de première référence.")
    else:
        lines.append(f"{len(results['ecrans'])} écran(s) comparé(s) à la version précédente, "
                     f"{len(alerts)} changement(s) important(s) (≥ {args.seuil:g} % des pixels).")
    if alerts:
        lines += ["", "| Écran | Pixels changés |", "|---|---|"]
        lines += [f"| ⚠️ {k} | {v:.1f} % |" for k, v in sorted(alerts.items(), key=lambda kv: -kv[1])]
    if small:
        lines += ["", "Petits changements (horloge, anticrénelage…) : "
                  + ", ".join(f"{k} {v:.2f} %" for k, v in sorted(small.items()))]
    if results["nouveaux"]:
        lines += ["", "Nouveaux écrans : " + ", ".join(results["nouveaux"])]
    if results["disparus"]:
        lines += ["", "⚠️ Écrans disparus : " + ", ".join(results["disparus"])]
        if github:
            print("::warning title=Captures disparues::" + ", ".join(results["disparus"]))
    lines += ["", "Images : artefacts `captures-ui` (nouvelle série) et `captures-ui-diff` (pixels changés en rouge)."]
    (output / "rapport.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (output / "rapport.json").write_text(json.dumps(results, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
