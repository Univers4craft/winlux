#!/usr/bin/python3
"""Rend l'identité visuelle Winlux (logo « Prisme », fonds, thème GRUB).

Tout est calculé ici : aucune image tierce. Le texte des images utilise la
police Inter (SIL Open Font License 1.1) si elle est installée, sinon
DejaVu Sans (licence libre Bitstream Vera). Relancer après une retouche :

    python3 scripts/render-brand.py

Les PNG produits sont versionnés dans assets/brand/ et assets/wallpapers/ ;
la construction de l'ISO ne fait que les copier.
"""
from pathlib import Path
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "assets" / "brand"
WALL = ROOT / "assets" / "wallpapers"

# Palette Winlux : violet (Linux, la nuit) vers cyan (la lumière, « lux »).
VIOLET_LIGHT = (0xB6, 0x9C, 0xFF)
VIOLET = (0x7C, 0x4D, 0xFF)
VIOLET_DEEP = (0x4B, 0x23, 0xC8)
CYAN_LIGHT = (0x7D, 0xE8, 0xFF)
CYAN = (0x22, 0xB8, 0xF0)
BLUE_DEEP = (0x12, 0x5C, 0xD6)
NIGHT = (0x07, 0x0A, 0x1F)

# Géométrie du logo dans une boîte 0..1 : losange dont le centre optique est
# remonté (CY) pour donner un relief de gemme. Le Rust (shell/brand.rs) et le
# SVG reprennent exactement ces valeurs.
CY = 0.42
FACETS = (
    # (nom, couleur haut, couleur bas)
    ("haut-gauche", VIOLET_LIGHT, VIOLET),
    ("bas-gauche", VIOLET, VIOLET_DEEP),
    ("haut-droite", CYAN_LIGHT, CYAN),
    ("bas-droite", CYAN, BLUE_DEEP),
)


def font(size, weight="SemiBold"):
    for path in (
        f"/usr/share/fonts/opentype/inter/Inter-{weight}.otf",
        f"/usr/share/fonts/truetype/inter/Inter-{weight}.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ):
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    sys.exit("police Inter ou DejaVu Sans introuvable")


def lerp(a, b, t):
    t = np.clip(t, 0, 1)[..., None]
    return np.asarray(a, float) * (1 - t) + np.asarray(b, float) * t


def logo_rgba(size, ss=4, mono=None):
    """Logo carré `size` px, suréchantillonné `ss` fois (bords lisses)."""
    n = size * ss
    y, x = np.mgrid[0:n, 0:n].astype(float)
    u = (x + 0.5) / n
    v = (y + 0.5) / n
    inside = np.abs(u - 0.5) + np.abs(v - 0.5) <= 0.5
    left = u < 0.5
    # Arête qui sépare facettes haute et basse : du centre (0.5, CY) au sommet
    # latéral (0 ou 1, 0.5).
    edge = CY + (0.5 - CY) * np.abs(u - 0.5) / 0.5
    top = v < edge
    rgb = np.zeros((n, n, 3))
    t = v
    for (name, c0, c1), mask in zip(
        FACETS,
        (left & top, left & ~top, ~left & top, ~left & ~top),
    ):
        rgb[mask] = lerp(c0, c1, t)[mask]
    # Reflet : la facette haut-gauche s'éclaircit vers son arête (lumière).
    glow = np.clip(1 - np.hypot(u - 0.42, v - 0.30) / 0.30, 0, 1) ** 2
    rgb = rgb + (255 - rgb) * (glow * 0.35)[..., None] * (left & top)[..., None]
    if mono is not None:
        rgb[:] = mono
    alpha = inside.astype(float) * 255
    img = np.dstack([rgb, alpha]).clip(0, 255).astype(np.uint8)
    return Image.fromarray(img, "RGBA").resize((size, size), Image.LANCZOS)


def logo_svg(mono=None):
    def stop(c):
        return "#%02X%02X%02X" % c

    cy = 256 * CY
    pts = {
        "t": (128, 0),
        "r": (256, 128),
        "b": (128, 256),
        "l": (0, 128),
        "c": (128, cy),
    }
    tri = {
        "haut-gauche": ("t", "c", "l"),
        "bas-gauche": ("l", "c", "b"),
        "haut-droite": ("t", "r", "c"),
        "bas-droite": ("c", "r", "b"),
    }
    defs, shapes = [], []
    for i, (name, c0, c1) in enumerate(FACETS):
        d = " ".join("%g,%g" % pts[k] for k in tri[name])
        if mono:
            fill = mono
        else:
            defs.append(
                f'<linearGradient id="f{i}" x1="0" y1="0" x2="0" y2="256" '
                f'gradientUnits="userSpaceOnUse"><stop stop-color="{stop(c0)}"/>'
                f'<stop offset="1" stop-color="{stop(c1)}"/></linearGradient>'
            )
            fill = f"url(#f{i})"
        shapes.append(f'<polygon points="{d}" fill="{fill}"/>')
    if not mono:
        defs.append(
            '<radialGradient id="g" cx="107" cy="77" r="77" '
            'gradientUnits="userSpaceOnUse"><stop stop-color="#fff" '
            'stop-opacity=".35"/><stop offset="1" stop-color="#fff" '
            'stop-opacity="0"/></radialGradient>'
        )
        shapes.append(
            '<polygon points="128,0 %g,%g 0,128" fill="url(#g)"/>' % (128, cy)
        )
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" '
        'viewBox="0 0 256 256"><title>Winlux</title>'
        + ("<defs>%s</defs>" % "".join(defs) if defs else "")
        + "".join(shapes)
        + "</svg>\n"
    )


def background(w, h, light=False):
    """Fond « Prisme » : nuit violette (ou aube claire) et rubans de lumière."""
    y, x = np.mgrid[0:h, 0:w].astype(float)
    u, v = x / w, y / h
    a = h / w
    if light:
        base = lerp((0xF4, 0xF1, 0xFF), (0xD9, 0xEC, 0xFF), v * 0.7 + u * 0.3)
    else:
        base = lerp(NIGHT, (0x14, 0x10, 0x3A), v)
        base = lerp(base, (0x0B, 0x24, 0x4A), np.clip(u - 0.3, 0, 1) * 0.8)
    img = base
    # Halo central derrière le logo.
    r = np.hypot((u - 0.5), (v - 0.47) * a)
    halo = np.clip(1 - r / 0.42, 0, 1) ** 2.2
    img = img + (np.asarray((0x8E, 0x6B, 0xFF) if not light else (0xC9, 0xB8, 0xFF), float) - img) * (halo * (0.55 if not light else 0.5))[..., None]
    # Rubans : sinusoïdes douces, violet à gauche, cyan à droite.
    for k, (phase, amp, width, col, gain) in enumerate((
        (0.0, 0.10, 0.030, VIOLET, 0.55),
        (1.7, 0.08, 0.018, CYAN, 0.60),
        (3.1, 0.12, 0.045, VIOLET_LIGHT, 0.30),
        (4.4, 0.06, 0.012, CYAN_LIGHT, 0.70),
    )):
        center = 0.62 + amp * np.sin(u * 2 * np.pi * 0.9 + phase) + 0.05 * np.sin(u * 7 + k)
        d = np.abs(v - center)
        band = np.exp(-(d / width) ** 2)
        fade = np.clip(np.abs(u - 0.5) * 2.2, 0.15, 1)
        if light:
            img = img + (np.asarray(col, float) - img) * (band * gain * 0.55 * fade)[..., None]
        else:
            img = img + np.asarray(col, float) * (band * gain * 0.55 * fade)[..., None]
    # Grain léger contre les bandes de dégradé.
    rng = np.random.default_rng(7)
    img = img + rng.normal(0, 0.6, img.shape)
    return Image.fromarray(img.clip(0, 255).astype(np.uint8), "RGB")


def aurora_background(w, h, light=False):
    """Fond « Aurore » : rideaux de lumière violet → cyan au-dessus de
    collines, ciel étoilé la nuit. Tout est proportionnel à la hauteur, si
    bien que 1080p, 1440p et 4K donnent la même image."""
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    u, v = x / w, y / h
    del x, y
    if light:
        img = lerp((0xF7, 0xF5, 0xFF), (0xE0, 0xE8, 0xFF), v)
    else:
        img = lerp((0x05, 0x06, 0x18), (0x16, 0x10, 0x3E), v ** 1.3)
    # Halo très doux derrière le logo.
    r = np.hypot(u - 0.5, (v - 0.36) * h / w)
    halo = np.clip(1 - r / 0.30, 0, 1) ** 2
    img += (np.asarray((0xD9, 0xCC, 0xFF) if light else (0x3A, 0x26, 0x8C), float) - img) * (halo * 0.45)[..., None]
    # Rayons verticaux : mêmes fréquences en coordonnées relatives quelle que
    # soit la résolution (u * fréquence), phase irrégulière pour éviter l'effet
    # « code-barres ».
    curtains = (
        # base, amplitude, phase, intensité, hauteur des rayons
        (0.60, 0.065, 0.4, 1.00, 0.30),
        (0.69, 0.050, 2.1, 0.70, 0.22),
        (0.54, 0.040, 4.0, 0.35, 0.38),
    )
    for k, (y0, amp, ph, gain, hgt) in enumerate(curtains):
        base = y0 + amp * np.sin(u * 2 * np.pi * 0.7 + ph) + 0.02 * np.sin(u * 2 * np.pi * 2.3 + k)
        above = base - v
        prof = np.where(above > 0, np.exp(-np.maximum(above, 0) / hgt), np.exp(-(above / 0.012) ** 2))
        wobble = 3 * np.sin(u * 17 + k) + 1.5 * np.sin(u * 53 + 2 * k)
        rays = 0.55 + 0.45 * (0.5 + 0.5 * np.sin(u * 230 + wobble)) * (0.6 + 0.4 * np.sin(u * 41 + k))
        m = prof * rays * gain * np.clip(0.25 + 0.75 * np.sin(np.pi * u) ** 0.6, 0, 1)
        t = np.clip(above / 0.25, 0, 1)
        col = lerp(lerp(VIOLET, VIOLET_LIGHT, t), lerp(CYAN, CYAN_LIGHT, t), u)
        if light:
            img += (col - img) * (m * 0.55)[..., None]
        else:
            img += col * (m * 0.62)[..., None]
        del base, above, prof, rays, m, t, col
    if not light:
        # Étoiles fixes (même tirage, positions relatives) au-dessus des rideaux.
        rng = np.random.default_rng(3)
        n = 230
        su, sv = rng.uniform(0, 1, n), rng.uniform(0, 0.5, n) ** 1.2
        glow = rng.uniform(40, 150, n)
        rad = max(1, round(h / 1080))
        for a, b, g in zip(su, sv, glow):
            cx, cy = int(a * w), int(b * h)
            img[max(cy - rad + 1, 0):cy + rad, max(cx - rad + 1, 0):cx + rad] += g
    rng = np.random.default_rng(7)
    img += rng.normal(0, 0.6, img.shape).astype(np.float32)
    return Image.fromarray(img.clip(0, 255).astype(np.uint8), "RGB")


def aurora_wallpaper(w, h, light):
    img = aurora_background(w, h, light).convert("RGBA")
    paste_logo(img, int(h * 0.16), w // 2, int(h * 0.36), light=light)
    return img.convert("RGB")


# Fond « Aurore » (#596) : sombre par défaut, clair, en trois définitions. Le
# 1080p garde le nom court ; Paramètres et la session prennent la définition
# la plus proche de l'écran.
AURORA_SIZES = ((1920, 1080, ""), (2560, 1440, "-2560x1440"), (3840, 2160, "-3840x2160"))
AURORA_NAMES = (("winlux-prisme-aurore", False), ("winlux-prisme-aurore-clair", True))


def paste_logo(img, size, cx, cy, shadow=True, light=False):
    logo = logo_rgba(size)
    if shadow:
        glow = Image.new("RGBA", img.size, (0, 0, 0, 0))
        g = logo_rgba(size, ss=1)
        tint = Image.new("RGBA", g.size, (0x9B, 0x7B, 0xFF, 0) if not light else (0x60, 0x40, 0xC0, 0))
        tint.putalpha(g.getchannel("A").point(lambda p: p * (0.55 if not light else 0.30)))
        glow.paste(tint, (cx - size // 2, cy - size // 2 + size // 10), tint)
        glow = glow.filter(ImageFilter.GaussianBlur(size / 6))
        img.paste(glow, (0, 0), glow)
    img.paste(logo, (cx - size // 2, cy - size // 2), logo)


def wordmark(img, text, size, cx, top, color):
    d = ImageDraw.Draw(img)
    f = font(size)
    w = d.textlength(text, font=f)
    d.text((cx - w / 2, top), text, font=f, fill=color)


def wallpaper(w, h, light):
    img = background(w, h, light).convert("RGBA")
    s = int(h * 0.20)
    paste_logo(img, s, w // 2, int(h * 0.44), light=light)
    return img.convert("RGB")


def grub_background(w=1920, h=1080):
    img = background(w, h).convert("RGBA")
    # Assombrit pour la lisibilité du menu.
    dark = Image.new("RGBA", (w, h), (4, 6, 18, 120))
    img = Image.alpha_composite(img, dark)
    s = 132
    paste_logo(img, s, w // 2, 190)
    wordmark(img, "Winlux", 54, w // 2, 270, (0xF2, 0xF0, 0xFF))
    return img.convert("RGB")


def grub_selection(directory, r=6):
    """Surbrillance arrondie de l'entrée choisie (9 morceaux pour GRUB)."""
    size = 2 * r + 1
    big = Image.new("RGBA", (size * 4, size * 4), (0, 0, 0, 0))
    ImageDraw.Draw(big).rounded_rectangle(
        (0, 0, size * 4 - 1, size * 4 - 1), radius=r * 4, fill=(0x7C, 0x4D, 0xFF, 110),
        outline=(0xB6, 0x9C, 0xFF, 170), width=4,
    )
    whole = big.resize((size, size), Image.LANCZOS)
    boxes = {
        "nw": (0, 0, r, r), "n": (r, 0, r + 1, r), "ne": (r + 1, 0, size, r),
        "w": (0, r, r, r + 1), "c": (r, r, r + 1, r + 1), "e": (r + 1, r, size, r + 1),
        "sw": (0, r + 1, r, size), "s": (r, r + 1, r + 1, size), "se": (r + 1, r + 1, size, size),
    }
    for name, box in boxes.items():
        whole.crop(box).save(directory / f"select_{name}.png")


def main():
    BRAND.mkdir(parents=True, exist_ok=True)
    (BRAND / "winlux-logo.svg").write_text(logo_svg())
    (BRAND / "winlux-logo-blanc.svg").write_text(logo_svg(mono="#FFFFFF"))
    for size in (16, 32, 48, 64, 128, 256, 512):
        logo_rgba(size).save(BRAND / f"winlux-logo-{size}.png", optimize=True)
    # Bannière (logo + nom) pour README, À propos et documentation.
    banner = Image.new("RGBA", (880, 240), (0, 0, 0, 0))
    paste_logo(banner, 176, 120, 120, shadow=False)
    d = ImageDraw.Draw(banner)
    d.text((240, 52), "Winlux", font=font(112), fill=(0x1B, 0x14, 0x3D))
    banner.save(BRAND / "winlux-banniere.png", optimize=True)
    grub = BRAND / "grub"
    grub.mkdir(exist_ok=True)
    grub_background().save(grub / "background.png", optimize=True)
    grub_selection(grub)
    for name, light in (("winlux-prisme-nuit", False), ("winlux-prisme-aube", True)):
        wallpaper(1920, 1080, light).save(WALL / f"{name}.png", optimize=True)
    for name, light in AURORA_NAMES:
        for w, h, suffix in AURORA_SIZES:
            aurora_wallpaper(w, h, light).save(WALL / f"{name}{suffix}.png", optimize=True)
    print("identité Winlux rendue dans", BRAND, "et", WALL)


if __name__ == "__main__":
    main()
