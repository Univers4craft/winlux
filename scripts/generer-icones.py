#!/usr/bin/env python3
"""Génère le jeu d'icônes Winlux « Prisme » (clair et sombre).

Deux dessins par icône :
  * grille 16 (petit) : traits alignés sur le pixel, pour 16, 20 et 24 px ;
  * grille 64 (grand) : détaillé, pour 32 px et au-delà, 4K compris.

Sorties :
  integration/native/icons/Winlux/          thème clair (SVG + PNG 16…256, alias en liens)
  integration/native/icons/Winlux-Sombre/   thème sombre (hérite de Winlux)
  <sortie>/explorateur/  PNG pour le shell PE (explorer.rs), <nom>-<taille>.png
  <sortie>/planche-clair.png, <sortie>/planche-sombre.png   planches de contrôle
<sortie> est le dossier passé en argument, par défaut target/icones-prisme.

Les icônes d'applications (scalable/apps, dont winlux.svg) restent dessinées
par scripts/render-icons.py : ce script les conserve et les déclare dans
l'index.theme du thème clair. Relancer :

    python3 scripts/generer-icones.py
    sh scripts/test-icones.sh integration/native/icons

Aucun élément de marque Microsoft ni Ubuntu : seul le logo Prisme est utilisé.
Rendu PNG par GdkPixbuf (librsvg), sans dépendance externe.
"""
import math, os, sys, shutil

ACC = {"clair": "#6A3DF0", "sombre": "#B69CFF"}

def palette(mode):
    d = mode == "sombre"
    return dict(
        mode=mode,
        acc="#6A3DF0" if not d else "#8F6BFF",      # accent posé sur papier / jaune
        acc_bg=ACC[mode],                           # accent posé sur le fond
        acc_dk="#4A22C8" if not d else "#6A3DF0",
        blue="#1A73E8" if not d else "#3D8BFF",
        green="#16924F" if not d else "#2FB872",
        orange="#E0601F" if not d else "#F07A3A",
        red="#D32F2F" if not d else "#E5484D",
        amber="#B86E00" if not d else "#C98A12",
        grey="#5C6372" if not d else "#6C7383",
        paper="#FFFFFF" if not d else "#F1F2F6",
        fold="#E4E7EF" if not d else "#C9CDD8",
        edge="#9DA3B2" if not d else "#C9CEDA",
        f_back="#E0A31C" if not d else "#D69A16",
        f_top="#FFD965" if not d else "#FCD25A",
        f_bot="#F6B62E" if not d else "#EFAA22",
        f_in="#FFF7E3",
        f_glyph="#8A4F00" if not d else "#7A4500",
        slab_t="#F1F3F7" if not d else "#E3E6EE",
        slab_b="#C3C9D5" if not d else "#A9B0BF",
        slab_e="#7F8697" if not d else "#DDE1EA",
        dark_body="#2C303B" if not d else "#3B404D",
        light_fill="#F4F1FD" if not d else "#E9E3FF",
        glyph=("#1F1F1F" if not d else "#F2F2F2"),  # glyphes monochromes de barre d'outils
    )

# --------------------------------------------------------------------------
# Glyphes sur boîte 16x16, en traits. sw = épaisseur dans les unités du glyphe.

def glyph(name, color, sw, fill=None):
    s = f'fill="none" stroke="{color}" stroke-width="{sw:.3f}" stroke-linecap="round" stroke-linejoin="round"'
    f = f'fill="{fill or color}"'
    G = {
        "bureau": f'<g {s}><rect x="2" y="3" width="12" height="8.5" rx="1.2"/><path d="M8 11.5V14M5.5 14h5"/></g>',
        "documents": f'<g {s}><path d="M4 1.8h5.5L12.5 5v9.2H4z"/><path d="M6.3 8h4M6.3 11h4"/></g>',
        "telechargements": f'<g {s}><path d="M8 2v9M4.2 7.4 8 11.2l3.8-3.8M3 14.3h10"/></g>',
        "images": f'<g {s}><rect x="1.6" y="2.8" width="12.8" height="10.4" rx="1.4"/><path d="M3.6 11.6 6.6 8l2.2 2.4 1.5-1.4 2.2 2.6"/></g><circle cx="10.6" cy="5.9" r="1.3" {f}/>',
        "musique": f'<g {s}><path d="M6 12.2V3.4l7.3-1.6v8.6"/></g><circle cx="4.1" cy="12.3" r="2.1" {f}/><circle cx="11.4" cy="10.5" r="2.1" {f}/>',
        "videos": f'<path d="M4.5 2.6v10.8L13.4 8z" {f} stroke="{color}" stroke-width="{sw*0.6:.3f}" stroke-linejoin="round"/>',
        "accueil": f'<g {s}><path d="M2 8 8 2.6 14 8M4 6.6V14h8V6.6M6.8 14v-3.6h2.4V14"/></g>',
        "reseau": f'<g {s}><circle cx="8" cy="8" r="6.2"/><ellipse cx="8" cy="8" rx="2.6" ry="6.2"/><path d="M1.8 8h12.4"/></g>',
        "recent": f'<g {s}><circle cx="8" cy="8" r="6.2"/><path d="M8 4.4V8l2.6 1.6"/></g>',
        "terminal": f'<g {s}><path d="M3 4l4 4-4 4M8.8 12.5h4.4"/></g>',
        "code": f'<g {s}><path d="M5 4 1.6 8 5 12M11 4l3.4 4L11 12M9.4 2.8 6.6 13.2"/></g>',
        "accolades": f'<g {s}><path d="M6 2.2C4.4 2.2 4.4 3 4.4 4.4v1.8C4.4 7.2 3.6 8 2.6 8c1 0 1.8.8 1.8 1.8v1.8c0 1.4 0 2.2 1.6 2.2M10 2.2c1.6 0 1.6.8 1.6 2.2v1.8c0 1 .8 1.8 1.8 1.8-1 0-1.8.8-1.8 1.8v1.8c0 1.4 0 2.2-1.6 2.2"/></g>',
        "lignes": f'<g {s}><path d="M3 3.5h10M3 6.5h10M3 9.5h10M3 12.5h6.5"/></g>',
        "tableur": f'<g {s}><rect x="2" y="2.5" width="12" height="11" rx="1"/><path d="M2 6.2h12M2 9.8h12M6.4 2.5v11"/></g>',
        "diapo": f'<g {s}><rect x="1.6" y="2.4" width="12.8" height="8.8" rx="1"/><path d="M8 11.2v3M5.6 14.2h4.8M5 8.6V7M8 8.6V5M11 8.6V6.2"/></g>',
        "zip": f'<g {s}><path d="M8 1v3M8 6v3"/></g><rect x="6.2" y="9.4" width="3.6" height="4.6" rx="0.9" {f}/>',
        "paquet": f'<g {s}><path d="M8 1.6 14 4.6v6.8L8 14.4 2 11.4V4.6zM2 4.6 8 7.6l6-3M8 7.6v6.8"/></g>',
        "police": f'<g {s}><path d="M2.8 14 8 2l5.2 12M4.9 9.6h6.2"/></g>',
        "disque-optique": f'<g {s}><circle cx="8" cy="8" r="6.3"/><circle cx="8" cy="8" r="1.8"/></g>',
        "ce-pc": f'<g {s}><rect x="1.6" y="2.4" width="12.8" height="9" rx="1.2"/><path d="M8 11.4v2.6M5.2 14h5.6"/></g>',
        # barre d'outils de l'Explorateur
        "couper": f'<g {s}><circle cx="4.6" cy="11.6" r="2.3"/><circle cx="11.4" cy="11.6" r="2.3"/><path d="M6.3 10 12.4 1.8M9.7 10 3.6 1.8"/></g>',
        "copier": f'<g {s}><rect x="5.4" y="5.4" width="8.6" height="8.6" rx="1.6"/><path d="M10.6 5.4V3.6A1.6 1.6 0 0 0 9 2H3.6A1.6 1.6 0 0 0 2 3.6V9a1.6 1.6 0 0 0 1.6 1.6h1.8"/></g>',
        "coller": f'<g {s}><path d="M5.4 3H4a1.4 1.4 0 0 0-1.4 1.4v8.8A1.4 1.4 0 0 0 4 14.6h8a1.4 1.4 0 0 0 1.4-1.4V4.4A1.4 1.4 0 0 0 12 3h-1.4"/><rect x="5.4" y="1.6" width="5.2" height="2.8" rx="0.8"/></g>',
        "renommer": f'<g {s}><path d="M10.4 2.6l3 3L6 13H3v-3zM8.8 4.2l3 3"/></g>',
        "supprimer": f'<g {s}><path d="M2.4 4h11.2M6 4V2.6h4V4M3.8 4l.8 9.4c.1.7.6 1.2 1.3 1.2h4.2c.7 0 1.2-.5 1.3-1.2L12.2 4M6.6 7v4.8M9.4 7v4.8"/></g>',
        "partager": f'<g {s}><path d="M9.6 2.4 13.8 6.4 9.6 10.4M13.6 6.4H8.4C5 6.4 2.4 9 2.4 12.4v1.2"/></g>',
        "trier": f'<g {s}><path d="M5 13.6V2.4M2.2 5.2 5 2.4l2.8 2.8M11 2.4v11.2M8.2 10.8l2.8 2.8 2.8-2.8"/></g>',
        "afficher": f'<g {s}><rect x="2" y="2" width="5" height="5" rx="1"/><rect x="9" y="2" width="5" height="5" rx="1"/><rect x="2" y="9" width="5" height="5" rx="1"/><rect x="9" y="9" width="5" height="5" rx="1"/></g>',
        "nouveau": f'<g {s}><path d="M8 2.5v11M2.5 8h11"/></g>',
        "rechercher": f'<g {s}><circle cx="6.8" cy="6.8" r="4.6"/><path d="M10.2 10.2 14 14"/></g>',
        "details": f'<g {s}><rect x="1.8" y="2.6" width="12.4" height="10.8" rx="1.6"/><path d="M9.6 2.6v10.8"/></g>',
        "options": f'<circle cx="3.2" cy="8" r="1.25" {f}/><circle cx="8" cy="8" r="1.25" {f}/><circle cx="12.8" cy="8" r="1.25" {f}/>',
        "chevron-bas": f'<g {s}><path d="M4 6.2 8 10.2l4-4"/></g>',
        "double-volet": f'<g {s}><rect x="1.8" y="2.6" width="12.4" height="10.8" rx="1.6"/><path d="M8 2.6v10.8"/></g>',
    }
    return G[name]

def place(g, cx, cy, box):
    k = box / 16
    return f'<g transform="translate({cx - box/2:.3f} {cy - box/2:.3f}) scale({k:.4f})">{g}</g>'

PRISME = ('<polygon points="32,6 6,32 32,26.9" fill="#B69CFF"/><polygon points="6,32 32,26.9 32,58" fill="#5E35E6"/>'
          '<polygon points="32,6 58,32 32,26.9" fill="#7DE8FF"/><polygon points="32,26.9 58,32 32,58" fill="#1A8AE6"/>')

def prisme(cx, cy, box):
    k = box / 52
    return f'<g transform="translate({cx - 32*k:.3f} {cy - 32*k:.3f}) scale({k:.4f})">{PRISME}</g>'

def svg(size, body, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">'
            f'{body}<title>{title}</title></svg>')

# --------------------------------------------------------------------------
# Formes de base

def dossier(p, big, glyphe=None, gcol=None):
    if big:
        b = (f'<defs><linearGradient id="fv" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{p["f_top"]}"/>'
             f'<stop offset="1" stop-color="{p["f_bot"]}"/></linearGradient></defs>'
             f'<path d="M5 14a4 4 0 0 1 4-4h14.3a4 4 0 0 1 2.9 1.2L30 15h25a4 4 0 0 1 4 4v33a4 4 0 0 1-4 4H9a4 4 0 0 1-4-4z" fill="{p["f_back"]}"/>'
             f'<rect x="9" y="19" width="46" height="22" rx="2" fill="{p["f_in"]}"/>'
             f'<path d="M5 26a3 3 0 0 1 3-3h48a3 3 0 0 1 3 3v26a4 4 0 0 1-4 4H9a4 4 0 0 1-4-4z" fill="url(#fv)"/>'
             f'<path d="M8 23.6h48" stroke="#FFFFFF" stroke-opacity=".55" stroke-width="1.2"/>')
        if glyphe:
            b += place(glyph(glyphe, gcol or p["f_glyph"], 2.1), 32, 39.5, 20)
        return b
    b = (f'<defs><linearGradient id="fv" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{p["f_top"]}"/>'
         f'<stop offset="1" stop-color="{p["f_bot"]}"/></linearGradient></defs>'
         f'<path d="M1 3.5a1 1 0 0 1 1-1h3.6l1.5 1.5H14a1 1 0 0 1 1 1V13a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1z" fill="{p["f_back"]}"/>'
         f'<rect x="2" y="5" width="12" height="3" fill="{p["f_in"]}"/>'
         f'<path d="M1 7a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1z" fill="url(#fv)"/>')
    if glyphe:
        b += place(glyph(glyphe, gcol or p["f_glyph"], 2.9), 8, 10, 5.6)
    return b

def page(p, big, glyphe=None, gcol=None, extra=""):
    if big:
        b = (f'<path d="M15 4.5h23.5L51.5 17.5V56a3.5 3.5 0 0 1-3.5 3.5H15A3.5 3.5 0 0 1 11.5 56V8A3.5 3.5 0 0 1 15 4.5z" fill="{p["paper"]}" stroke="{p["edge"]}"/>'
             f'<path d="M38.5 4.5V14a3.5 3.5 0 0 0 3.5 3.5h9.5" fill="{p["fold"]}" stroke="{p["edge"]}" stroke-linejoin="round"/>')
        if glyphe:
            b += place(glyph(glyphe, gcol, 1.9), 31.5, 38, 22)
        return b + extra
    b = (f'<path d="M3.5 1.5h6l3 3V14a.5.5 0 0 1-.5.5H4a.5.5 0 0 1-.5-.5V2a.5.5 0 0 1 .5-.5z" fill="{p["paper"]}" stroke="{p["edge"]}"/>'
         f'<path d="M9.5 1.5v3h3" fill="{p["fold"]}" stroke="{p["edge"]}" stroke-linejoin="round"/>')
    if glyphe:
        b += place(glyph(glyphe, gcol, 2.3), 8, 9.6, 7)
    return b + extra

def disque(p, big, badge=None):
    if big:
        b = (f'<defs><linearGradient id="dk" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{p["slab_t"]}"/>'
             f'<stop offset="1" stop-color="{p["slab_b"]}"/></linearGradient></defs>'
             f'<rect x="5.5" y="21.5" width="53" height="25" rx="5" fill="url(#dk)" stroke="{p["slab_e"]}"/>'
             f'<path d="M6 38.5h52" stroke="{p["slab_e"]}" stroke-opacity=".6"/>'
             f'<rect x="38" y="41" width="14" height="2" rx="1" fill="{p["grey"]}" opacity=".55"/>'
             f'<circle cx="13" cy="42" r="2" fill="#2FCB78"/>')
        if badge == "prisme":
            b += prisme(17, 30, 13)
        elif badge == "linux":
            b += (f'<rect x="10" y="24.5" width="13" height="11" rx="2.5" fill="{p["acc_bg"] if p["mode"]=="clair" else "#6A3DF0"}"/>'
                  f'<path d="M14 32.5l2.4-2.4L14 27.7M17.6 32.6h2.6" fill="none" stroke="#fff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>')
        return b
    b = (f'<defs><linearGradient id="dk" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{p["slab_t"]}"/>'
         f'<stop offset="1" stop-color="{p["slab_b"]}"/></linearGradient></defs>'
         f'<rect x="1.5" y="4.5" width="13" height="8" rx="1.5" fill="url(#dk)" stroke="{p["slab_e"]}"/>'
         f'<rect x="3" y="10" width="1.5" height="1" fill="#1FB765"/>')
    if badge == "prisme":
        b += prisme(5, 7, 4.6)
    elif badge == "linux":
        b += f'<rect x="3" y="6" width="4" height="3" rx=".6" fill="{p["acc_bg"] if p["mode"]=="clair" else "#6A3DF0"}"/>'
    return b

def ordinateur(p, big):
    if big:
        return (f'<defs><linearGradient id="ec" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#7B4DFF"/>'
                f'<stop offset=".6" stop-color="#4E5BF0"/><stop offset="1" stop-color="#23A6F0"/></linearGradient></defs>'
                f'<rect x="4.5" y="8.5" width="55" height="38" rx="4" fill="{p["dark_body"]}" stroke="{p["slab_e"]}" stroke-opacity=".7"/>'
                f'<rect x="7.5" y="11.5" width="49" height="32" rx="1.5" fill="url(#ec)"/>'
                f'<path d="M7.5 37c10-9 22-12 49-6v11a1.5 1.5 0 0 1-1.5 1.5H9A1.5 1.5 0 0 1 7.5 42z" fill="#fff" opacity=".18"/>'
                + prisme(32, 27.5, 14) +
                f'<path d="M27 47h10l1.5 7h-13z" fill="{p["slab_b"]}"/>'
                f'<rect x="19.5" y="53.5" width="25" height="4" rx="2" fill="{p["slab_t"]}" stroke="{p["slab_e"]}"/>')
    return (f'<defs><linearGradient id="ec" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#7B4DFF"/>'
            f'<stop offset="1" stop-color="#23A6F0"/></linearGradient></defs>'
            f'<rect x="1" y="2" width="14" height="10" rx="1" fill="{p["dark_body"]}"/>'
            f'<rect x="2" y="3" width="12" height="8" fill="url(#ec)"/>'
            f'<rect x="5" y="13" width="6" height="1.5" rx=".5" fill="{p["slab_b"]}"/><rect x="7" y="12" width="2" height="1" fill="{p["slab_b"]}"/>')

def corbeille(p, big, pleine=False):
    if big:
        b = (f'<defs><linearGradient id="cb" x1="0" y1="0" x2="1" y2="0"><stop stop-color="#E9EEF7"/>'
             f'<stop offset=".5" stop-color="#FFFFFF"/><stop offset="1" stop-color="#C8D1E2"/></linearGradient></defs>')
        if pleine:
            b += (f'<path d="M19 23 24 9l12 5-5 11z" fill="#FFFFFF" stroke="{p["edge"]}"/>'
                  f'<path d="M30 24l9-15 9 6-6 10z" fill="{p["acc_bg"]}" opacity=".9"/>'
                  f'<path d="M24 24c2-6 9-8 14-5" fill="none" stroke="{p["edge"]}" stroke-width="1.4"/>')
        b += (f'<path d="M14.5 21.5h35l-3.3 34.8a4 4 0 0 1-4 3.7H21.8a4 4 0 0 1-4-3.7z" fill="url(#cb)" stroke="{p["edge"]}"/>'
              f'<path d="M25.5 28.5v24M32 28.5v24M38.5 28.5v24" stroke="{p["edge"]}" stroke-width="1.6" stroke-linecap="round" opacity=".8"/>')
        lid = (f'<rect x="10.5" y="15.5" width="43" height="7" rx="3.5" fill="{p["slab_t"]}" stroke="{p["edge"]}"/>'
               f'<path d="M26.5 15.5V13a2 2 0 0 1 2-2h7a2 2 0 0 1 2 2v2.5" fill="none" stroke="{p["edge"]}" stroke-width="1.6"/>')
        if pleine:
            lid = f'<g transform="rotate(-14 10 22)">{lid}</g>'
        return b + lid
    b = ""
    if pleine:
        b += f'<path d="M5 6 6.5 1.5l3.5 1.5L9 6z" fill="#fff" stroke="{p["edge"]}" stroke-width=".8"/><path d="M8 6l2.5-4.5 2.5 2L11.5 6z" fill="{p["acc_bg"]}"/>'
    b += (f'<path d="M3.5 5.5h9l-.85 8.6a1 1 0 0 1-1 .9h-5.3a1 1 0 0 1-1-.9z" fill="#F3F6FB" stroke="{p["edge"]}"/>'
          f'<path d="M6.5 7.5v5M9.5 7.5v5" stroke="{p["edge"]}"/>')
    lid = f'<rect x="2" y="4" width="12" height="2" rx="1" fill="{p["slab_t"]}" stroke="{p["edge"]}" stroke-width=".8"/>'
    if pleine:
        lid = f'<g transform="rotate(-12 2 6)">{lid}</g>'
    return b + lid

def maison(p, big):
    if big:
        return (f'<defs><linearGradient id="tt" x1="0" y1="0" x2="0" y2="1"><stop stop-color="#9273FF"/>'
                f'<stop offset="1" stop-color="#5A2EE6"/></linearGradient></defs>'
                f'<rect x="13.5" y="27.5" width="37" height="30" rx="3" fill="{p["light_fill"]}" stroke="{p["edge"]}"/>'
                f'<path d="M5.2 29.4 29.4 8.6a4 4 0 0 1 5.2 0l24.2 20.8a2 2 0 0 1-1.3 3.6H6.5a2 2 0 0 1-1.3-3.6z" fill="url(#tt)"/>'
                f'<rect x="27" y="40" width="10" height="17.5" rx="2" fill="#5A2EE6"/>'
                f'<rect x="40" y="38" width="6" height="6" rx="1" fill="#7DE8FF"/>')
    return (f'<path d="M3.5 7.5h9v6a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1z" fill="{p["light_fill"]}" stroke="{p["edge"]}"/>'
            f'<path d="M1.3 8.2 7.3 2.6a1 1 0 0 1 1.4 0l6 5.6c.3.3.1.8-.3.8H1.6c-.4 0-.6-.5-.3-.8z" fill="#7048F5"/>'
            f'<rect x="7" y="10.5" width="2" height="4" fill="#5A2EE6"/>')

def horloge(p, big):
    if big:
        return (f'<circle cx="32" cy="32" r="26" fill="{p["light_fill"]}"/>'
                f'<circle cx="32" cy="32" r="24.5" fill="none" stroke="{p["acc_bg"] if p["mode"]=="clair" else "#8F6BFF"}" stroke-width="4"/>'
                f'<path d="M32 16v16l10 6" fill="none" stroke="#4A22C8" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>'
                f'<circle cx="32" cy="32" r="3" fill="#4A22C8"/>')
    return (f'<circle cx="8" cy="8" r="6.5" fill="{p["light_fill"]}" stroke="#7048F5" stroke-width="1.5"/>'
            f'<path d="M8 4.5V8l2.5 1.5" fill="none" stroke="#4A22C8" stroke-width="1.5" stroke-linecap="round"/>')

def globe(p, big):
    c = "#3D8BFF" if p["mode"] == "sombre" else "#1A73E8"
    if big:
        return (f'<defs><radialGradient id="gb" cx=".35" cy=".3" r=".8"><stop stop-color="#9FD8FF"/>'
                f'<stop offset=".6" stop-color="#4E8BF5"/><stop offset="1" stop-color="#6A3DF0"/></radialGradient></defs>'
                f'<circle cx="32" cy="32" r="26" fill="url(#gb)"/>'
                + place(glyph("reseau", "#FFFFFF", 0.9), 32, 32, 52).replace('stroke-opacity', 'x') )
    return f'<circle cx="8" cy="8" r="6.5" fill="{c}"/>' + place(glyph("reseau", "#FFFFFF", 1.2), 8, 8, 13)

def executable(p, big):
    if big:
        return (f'<rect x="6.5" y="9.5" width="51" height="45" rx="6" fill="{p["paper"]}" stroke="{p["edge"]}"/>'
                f'<path d="M6.5 15.5a6 6 0 0 1 6-6h39a6 6 0 0 1 6 6v3h-51z" fill="{p["acc"]}"/>'
                f'<circle cx="46" cy="14" r="1.6" fill="#fff"/><circle cx="51" cy="14" r="1.6" fill="#fff"/>'
                + prisme(32, 36.5, 22))
    return (f'<rect x="1.5" y="2.5" width="13" height="11" rx="1.5" fill="{p["paper"]}" stroke="{p["edge"]}"/>'
            f'<path d="M1.5 4a1.5 1.5 0 0 1 1.5-1.5h10A1.5 1.5 0 0 1 14.5 4v1h-13z" fill="{p["acc"]}"/>'
            + prisme(8, 9.5, 6.4))

def cle_usb(p, big):
    if big:
        return (f'<rect x="24.5" y="4.5" width="15" height="12" rx="1.5" fill="{p["slab_t"]}" stroke="{p["slab_e"]}"/>'
                f'<rect x="28" y="8" width="3" height="3" fill="{p["grey"]}"/><rect x="33" y="8" width="3" height="3" fill="{p["grey"]}"/>'
                f'<rect x="20" y="16" width="24" height="44" rx="5" fill="{p["acc_dk"]}"/>'
                f'<rect x="20" y="16" width="24" height="44" rx="5" fill="url(#ub)"/>'
                f'<defs><linearGradient id="ub" x1="0" y1="0" x2="1" y2="0"><stop stop-color="#8F6BFF"/><stop offset="1" stop-color="#4A22C8"/></linearGradient></defs>'
                + prisme(32, 40, 12))
    return (f'<rect x="6" y="1.5" width="4" height="3" fill="{p["slab_t"]}" stroke="{p["slab_e"]}" stroke-width=".8"/>'
            f'<rect x="4.5" y="4.5" width="7" height="10.5" rx="1.5" fill="#6A3DF0"/>')

def lecteur_optique(p, big):
    return disque(p, big) + (
        f'<path d="M20 30.5h24" stroke="{p["grey"]}" stroke-width="2" stroke-linecap="round"/>' if big else
        f'<path d="M5 7.5h6" stroke="{p["grey"]}"/>')

def disque_optique(p, big):
    if big:
        return (f'<defs><linearGradient id="cd" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#E6E9F2"/>'
                f'<stop offset=".35" stop-color="#C9B8FF"/><stop offset=".5" stop-color="#F5F6FA"/>'
                f'<stop offset=".7" stop-color="#9FE6F7"/><stop offset="1" stop-color="#D2D7E3"/></linearGradient></defs>'
                f'<circle cx="32" cy="32" r="27" fill="url(#cd)" stroke="{p["slab_e"]}"/>'
                f'<circle cx="32" cy="32" r="9" fill="{p["slab_t"]}" stroke="{p["slab_e"]}"/>'
                f'<circle cx="32" cy="32" r="3.5" fill="none" stroke="{p["slab_e"]}" stroke-width="1"/>')
    return (f'<circle cx="8" cy="8" r="6.5" fill="#D9D3F5" stroke="{p["slab_e"]}"/>'
            f'<circle cx="8" cy="8" r="2" fill="{p["slab_t"]}" stroke="{p["slab_e"]}"/>')

def image_iso(p, big):
    # disque posé sur une page
    if big:
        return page(p, True) + '<g transform="translate(14.5 22) scale(.55)">' + disque_optique(p, True).replace('id="cd"', 'id="cd2"').replace('url(#cd)', 'url(#cd2)') + '</g>'
    return page(p, False) + f'<circle cx="8" cy="9.8" r="3.3" fill="#C9B8FF" stroke="{p["slab_e"]}" stroke-width=".8"/><circle cx="8" cy="9.8" r=".9" fill="{p["paper"]}"/>'

def pdf(p, big):
    if big:
        return page(p, True, "lignes", p["grey"]) + (
            f'<rect x="6.5" y="40" width="30" height="15" rx="3" fill="{p["red"]}"/>'
            # « PDF » en tracés (pas de police nécessaire)
            f'<g fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
            f'<path d="M11 51v-7h2.6a2 2 0 0 1 0 4H11"/><path d="M18.5 51v-7h1.8a3.5 3.5 0 0 1 0 7z"/><path d="M27.5 51v-7h4M27.5 47.3h3"/></g>')
    return page(p, False) + f'<rect x="2" y="9" width="8" height="5" rx="1" fill="{p["red"]}"/><path d="M4 10.8h4M4 12.4h3" stroke="#fff" stroke-width=".9"/>'

def archive(p, big):
    if big:
        b = page(p, True)
        b += f'<rect x="27" y="4.5" width="9" height="55" fill="{p["amber"]}" opacity=".85"/>'
        for i, y in enumerate(range(8, 34, 4)):
            x = 27 if i % 2 == 0 else 32
            b += f'<rect x="{x}" y="{y}" width="4" height="2" fill="#fff" opacity=".9"/>'
        b += f'<rect x="26" y="36" width="11" height="13" rx="2.5" fill="{p["paper"]}" stroke="{p["amber"]}" stroke-width="2"/><rect x="29.5" y="40" width="4" height="5" rx="1" fill="{p["amber"]}"/>'
        return b
    return page(p, False) + (f'<rect x="7" y="2" width="2" height="12" fill="{p["amber"]}"/>'
                              f'<rect x="6" y="9" width="4" height="4" rx=".8" fill="{p["amber"]}"/><rect x="7.4" y="10.4" width="1.2" height="1.4" fill="#fff"/>')

def paquet(p, big):
    if big:
        return (f'<path d="M32 6 56 18v28L32 58 8 46V18z" fill="#C79A5E"/>'
                f'<path d="M8 18l24 12 24-12-24-12z" fill="#E2BC83"/>'
                f'<path d="M32 30v28l24-12V18z" fill="#A87B42"/>'
                f'<path d="M20 12l24 12v8l-4-2v-6L16 14z" fill="{p["acc_bg"] if p["mode"]=="clair" else "#8F6BFF"}" opacity=".95"/>')
    return (f'<path d="M8 1.5 14.5 4.7v6.6L8 14.5 1.5 11.3V4.7z" fill="#C79A5E"/>'
            f'<path d="M1.5 4.7 8 8l6.5-3.3L8 1.5z" fill="#E2BC83"/><path d="M8 8v6.5l6.5-3.2V4.7z" fill="#A87B42"/>'
            f'<path d="M5 3l6.5 3.3v1.6L10.3 7.3V6.6L3.8 3.6z" fill="#7048F5"/>')

# --------------------------------------------------------------------------
# Catalogue : nom -> (contexte, fonction(p, big), titre, alias)

def F(glyphe, couleur=None):
    return lambda p, big: dossier(p, big, glyphe, (p[couleur] if couleur else None))

def P(glyphe, couleur):
    return lambda p, big: page(p, big, glyphe, p[couleur])

ICONES = {
    "folder":            ("places", F(None), "Dossier", ["inode-directory", "folder-open", "folder-new"]),
    "user-desktop":      ("places", F("bureau"), "Bureau", ["folder-desktop"]),
    "folder-documents":  ("places", F("documents"), "Documents", []),
    "folder-download":   ("places", F("telechargements"), "Téléchargements", ["folder-downloads"]),
    "folder-pictures":   ("places", F("images"), "Images", ["folder-images"]),
    "folder-music":      ("places", F("musique"), "Musique", []),
    "folder-videos":     ("places", F("videos"), "Vidéos", ["folder-video"]),
    "folder-remote":     ("places", F("reseau"), "Dossier réseau", ["folder-publicshare"]),
    "folder-templates":  ("places", F("documents"), "Modèles", []),
    "user-home":         ("places", maison, "Accueil", ["go-home", "user-bookmarks"]),
    "user-trash":        ("places", corbeille, "Corbeille", ["trashcan_empty"]),
    "user-trash-full":   ("places", lambda p, b: corbeille(p, b, True), "Corbeille (pleine)", ["trashcan_full"]),
    "document-open-recent": ("places", horloge, "Récent", ["folder-recent", "document-recent"]),
    "network-workgroup": ("places", globe, "Réseau", ["network-server", "folder-network", "network"]),
    "start-here":        ("places", lambda p, big: PRISME if big else prisme(8, 8, 15), "Winlux", []),
    "computer":          ("devices", ordinateur, "Ce PC", ["user-desktop-computer", "computer-laptop"]),
    "drive-harddisk":    ("devices", disque, "Disque", ["drive-harddisk-solidstate", "drive-multidisk"]),
    "drive-harddisk-system": ("devices", lambda p, b: disque(p, b, "prisme"), "Disque système (C:)", ["drive-harddisk-root"]),
    "drive-harddisk-linux":  ("devices", lambda p, b: disque(p, b, "linux"), "Système Linux (Z:)", []),
    "drive-removable-media": ("devices", cle_usb, "Clé USB", ["drive-removable-media-usb", "media-flash", "media-removable"]),
    "drive-optical":     ("devices", lecteur_optique, "Lecteur optique", []),
    "media-optical":     ("devices", disque_optique, "Disque optique", ["media-optical-cd", "media-optical-dvd"]),
    "text-x-generic":    ("mimetypes", P("lignes", "grey"), "Texte", ["text-plain", "text-x-readme", "text-x-log"]),
    "text-x-script":     ("mimetypes", P("terminal", "acc"), "Script", ["application-x-shellscript", "text-x-python", "application-x-perl", "text-x-python3"]),
    "text-x-source":     ("mimetypes", P("code", "acc"), "Code source", ["text-rust", "text-x-c", "text-x-csrc", "text-x-chdr", "text-x-c++src", "text-x-java", "text-x-makefile", "application-javascript", "text-css"]),
    "text-html":         ("mimetypes", P("reseau", "blue"), "Page web", ["application-xhtml+xml"]),
    "application-json":  ("mimetypes", P("accolades", "amber"), "Données JSON/XML", ["application-xml", "text-xml", "application-x-yaml"]),
    "application-pdf":   ("mimetypes", pdf, "PDF", []),
    "image-x-generic":   ("mimetypes", P("images", "blue"), "Image", ["image-png", "image-jpeg", "image-gif", "image-webp", "image-bmp", "image-svg+xml", "image-tiff", "image-x-icon", "image-vnd.microsoft.icon"]),
    "audio-x-generic":   ("mimetypes", P("musique", "orange"), "Audio", ["audio-mpeg", "audio-flac", "audio-x-wav", "audio-ogg", "audio-x-vorbis+ogg", "audio-mp4"]),
    "video-x-generic":   ("mimetypes", P("videos", "acc"), "Vidéo", ["video-mp4", "video-x-matroska", "video-webm", "video-x-msvideo", "video-quicktime", "video-mpeg"]),
    "x-office-document": ("mimetypes", P("documents", "blue"), "Document", ["application-vnd.openxmlformats-officedocument.wordprocessingml.document", "application-msword", "application-vnd.oasis.opendocument.text", "application-rtf"]),
    "x-office-spreadsheet": ("mimetypes", P("tableur", "green"), "Tableur", ["application-vnd.openxmlformats-officedocument.spreadsheetml.sheet", "application-vnd.ms-excel", "application-vnd.oasis.opendocument.spreadsheet", "text-csv"]),
    "x-office-presentation": ("mimetypes", P("diapo", "orange"), "Présentation", ["application-vnd.openxmlformats-officedocument.presentationml.presentation", "application-vnd.ms-powerpoint", "application-vnd.oasis.opendocument.presentation"]),
    "font-x-generic":    ("mimetypes", P("police", "acc"), "Police", ["font-ttf", "font-otf", "font-woff", "font-woff2", "application-x-font-ttf"]),
    "package-x-generic": ("mimetypes", paquet, "Paquet", ["application-vnd.debian.binary-package", "application-x-deb", "application-x-rpm", "application-x-flatpak"]),
    "application-x-archive": ("mimetypes", archive, "Archive", ["application-zip", "application-x-7z-compressed", "application-x-rar", "application-vnd.rar", "application-x-tar", "application-gzip", "application-x-compressed-tar", "application-x-xz-compressed-tar", "application-x-bzip-compressed-tar", "application-x-zstd-compressed-tar"]),
    "application-x-executable": ("mimetypes", executable, "Programme", ["application-vnd.microsoft.portable-executable", "application-x-ms-dos-executable", "application-x-msdownload", "application-x-msi", "application-x-dosexec", "application-x-sharedlib", "application-x-pie-executable"]),
    "application-octet-stream": ("mimetypes", lambda p, big: page(p, big), "Fichier de type inconnu", ["unknown", "application-x-generic", "text-x-generic-template"]),
    "application-x-cd-image": ("mimetypes", image_iso, "Image disque", ["application-vnd.efi.iso", "application-x-iso9660-image", "application-vnd.efi.img", "application-x-raw-disk-image"]),
}

# Noms attendus par l'Explorateur PE (issue #589) -> icône du thème ou glyphe de barre d'outils
EXPLORATEUR = {
    "dossier": "folder", "corbeille-vide": "user-trash", "corbeille-pleine": "user-trash-full",
    "disque-c": "drive-harddisk-system", "disque-z": "drive-harddisk-linux",
    "bureau": "user-desktop", "documents": "folder-documents", "images": "folder-pictures",
    "musique": "folder-music", "videos": "folder-videos", "telechargements": "folder-download",
    "ce-pc": "computer", "accueil": "user-home", "fichier-inconnu": "application-octet-stream",
    "reseau": "network-workgroup", "lecteur-amovible": "drive-removable-media", "lecteur-reseau": "folder-remote",
    "type-pdf": "application-pdf", "type-archive": "application-x-archive", "type-audio": "audio-x-generic",
    "type-video": "video-x-generic", "type-image": "image-x-generic", "type-exe": "application-x-executable",
    "type-iso": "application-x-cd-image",
    # types de fichiers (#890) : un motif par famille, choisi à l'extension
    "type-texte": "text-x-generic", "type-script": "text-x-script", "type-code": "text-x-source",
    "type-web": "text-html", "type-document": "x-office-document", "type-tableur": "x-office-spreadsheet",
    "type-presentation": "x-office-presentation", "type-police": "font-x-generic",
}
GLYPHES_BARRE = ["couper", "copier", "coller", "renommer", "supprimer", "partager", "trier", "afficher", "nouveau",
                 "terminal", "rechercher", "details", "options", "chevron-bas", "double-volet"]

TAILLES_PNG = [16, 20, 24, 32, 48, 64, 96, 128, 256]
PETIT_MAX = 24  # jusqu'à 24 px on prend le dessin sur grille 16

def dessin(nom, mode, big):
    ctx, fn, titre, _ = ICONES[nom]
    p = palette(mode)
    return svg(64 if big else 16, fn(p, big), titre)

def glyphe_svg(nom, mode):
    p = palette(mode)
    return svg(16, glyph(nom, p["glyph"], 1.3), nom)

# --------------------------------------------------------------------------
import gi
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf

def rendre(svgtxt, taille, chemin):
    loader = GdkPixbuf.PixbufLoader.new_with_type("svg")
    loader.set_size(taille, taille)
    loader.write(svgtxt.encode())
    loader.close()
    loader.get_pixbuf().savev(chemin, "png", [], [])

def index_theme(nom, commentaire, herite, apps=False):
    # Les applications (render-icons.py) ne vivent que dans le thème clair,
    # dont le thème sombre hérite.
    dirs = ["scalable/apps"] if apps else []
    for ctx in ("places", "devices", "mimetypes"):
        dirs.append(f"16/{ctx}")
        dirs.append(f"scalable/{ctx}")
        for t in TAILLES_PNG:
            dirs.append(f"{t}x{t}/{ctx}")
    ctxname = {"places": "Places", "devices": "Devices", "mimetypes": "MimeTypes", "apps": "Applications"}
    out = [f"[Icon Theme]\nName={nom}\nComment={commentaire}\nInherits={herite}\nExample=folder\n"
           f"Directories={','.join(dirs)}\n"]
    for d in dirs:
        base, ctx = d.split("/")
        if d == "scalable/apps":
            out.append(f"[{d}]\nSize=64\nMinSize=16\nMaxSize=512\nType=Scalable\nContext={ctxname[ctx]}\n")
        elif base == "16":
            out.append(f"[{d}]\nSize=16\nMinSize=16\nMaxSize=24\nType=Scalable\nContext={ctxname[ctx]}\n")
        elif base == "scalable":
            out.append(f"[{d}]\nSize=64\nMinSize=25\nMaxSize=512\nType=Scalable\nContext={ctxname[ctx]}\n")
        else:
            t = int(base.split("x")[0])
            out.append(f"[{d}]\nSize={t}\nType=Fixed\nContext={ctxname[ctx]}\n")
    return "\n".join(out)

def lien(dossier, cible, alias):
    if alias == os.path.splitext(cible)[0] or alias + os.path.splitext(cible)[1] == cible:
        return  # un alias identique au nom écraserait le fichier par une boucle
    l = os.path.join(dossier, alias)
    if os.path.lexists(l):
        os.remove(l)
    os.symlink(cible, l)

def generer_theme(racine, nom_theme, mode):
    d = os.path.join(racine, nom_theme)
    # Tout est régénéré, sauf scalable/apps (icônes d'applications de render-icons.py).
    if os.path.isdir(d):
        for entree in os.listdir(d):
            chemin = os.path.join(d, entree)
            if entree == "scalable":
                for sous in os.listdir(chemin):
                    if sous != "apps":
                        shutil.rmtree(os.path.join(chemin, sous))
            elif os.path.isdir(chemin) and not os.path.islink(chemin):
                shutil.rmtree(chemin)
            else:
                os.remove(chemin)
    apps = os.path.isdir(os.path.join(d, "scalable", "apps"))
    commentaire = {"clair": "Icônes Winlux « Prisme », mode clair",
                   "sombre": "Icônes Winlux « Prisme », mode sombre"}[mode]
    herite = "Adwaita,hicolor" if mode == "clair" else "Winlux,Adwaita,hicolor"
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "index.theme"), "w") as f:
        f.write(index_theme(nom_theme, commentaire, herite, apps))
    for nom, (ctx, fn, titre, alias) in ICONES.items():
        grand, petit = dessin(nom, mode, True), dessin(nom, mode, False)
        for sous, contenu in (("scalable", grand), ("16", petit)):
            dd = os.path.join(d, sous, ctx); os.makedirs(dd, exist_ok=True)
            with open(os.path.join(dd, nom + ".svg"), "w") as f:
                f.write(contenu)
            for a in alias:
                lien(dd, nom + ".svg", a + ".svg")
        for t in TAILLES_PNG:
            dd = os.path.join(d, f"{t}x{t}", ctx); os.makedirs(dd, exist_ok=True)
            rendre(petit if t <= PETIT_MAX else grand, t, os.path.join(dd, nom + ".png"))
            for a in alias:
                lien(dd, nom + ".png", a + ".png")
    # Le logo et les icônes d'applications existants sont repris tels quels par
    # le paquet (scalable/apps) : ce script ne s'occupe que du système.

def generer_explorateur(racine):
    d = os.path.join(racine, "explorateur")
    shutil.rmtree(d, ignore_errors=True); os.makedirs(d)
    for mode, suffixe in (("sombre", ""), ("clair", "-clair")):
        for court, nom in EXPLORATEUR.items():
            for t in (16, 20, 24, 32, 48):
                rendre(dessin(nom, mode, t > PETIT_MAX), t, os.path.join(d, f"{court}{suffixe}-{t}.png"))
        for g in GLYPHES_BARRE:
            for t in (16, 20, 24, 32, 48):
                rendre(glyphe_svg(g, mode), t, os.path.join(d, f"{g}{suffixe}-{t}.png"))

def planche(racine, mode):
    import cairo
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    tailles = [16, 20, 24, 32, 48, 96]
    noms = list(ICONES)
    lw, rh = 200, 108
    W = lw + sum(t + 18 for t in tailles) + 20
    H = 60 + rh * 0 + len(noms) * (max(tailles) + 14) + 20
    surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, W, H)
    cr = cairo.Context(surf)
    bg, fg = ((0.953, 0.953, 0.953), (0.1, 0.1, 0.1)) if mode == "clair" else ((0.125, 0.125, 0.14), (0.93, 0.93, 0.93))
    cr.set_source_rgb(*bg); cr.paint()
    cr.set_source_rgb(*fg); cr.select_font_face("Sans", 0, 1); cr.set_font_size(18)
    cr.move_to(16, 32); cr.show_text(f"Winlux Prisme, mode {mode} : 16, 20, 24, 32, 48, 96 px")
    cr.select_font_face("Sans", 0, 0); cr.set_font_size(13)
    tmp = os.path.join(racine, ".tmp.png")
    y = 56
    for nom in noms:
        rowh = max(tailles) + 14
        cr.set_source_rgb(*fg); cr.move_to(16, y + rowh / 2 + 4); cr.show_text(ICONES[nom][2])
        x = lw
        for t in tailles:
            rendre(dessin(nom, mode, t > PETIT_MAX), t, tmp)
            pb = GdkPixbuf.Pixbuf.new_from_file(tmp)
            Gdk.cairo_set_source_pixbuf(cr, pb, x, y + (rowh - t) / 2)
            cr.paint()
            x += t + 18
        y += rowh
    os.remove(tmp)
    surf.write_to_png(os.path.join(racine, f"planche-{mode}.png"))

if __name__ == "__main__":
    depot = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    racine = sys.argv[1] if len(sys.argv) > 1 else os.path.join(depot, "target", "icones-prisme")
    os.makedirs(racine, exist_ok=True)
    themes = os.path.join(depot, "integration", "native", "icons")
    generer_theme(themes, "Winlux", "clair")
    generer_theme(themes, "Winlux-Sombre", "sombre")
    generer_explorateur(racine)
    planche(racine, "clair")
    planche(racine, "sombre")
    print("ok", racine)
