#!/usr/bin/python3
"""winlux-webview : put_UserAgent ne laisse jamais WebKit refuser la chaîne (#2611).

1. user_agent_header garde une chaîne déjà conforme à la grammaire de
   l'en-tête User-Agent (RFC 9110, §10.1.5), et réécrit mot à mot celles qui
   ne le sont pas (version « 1951/49 », espace en tête, « ; » hors
   commentaire, non-ASCII, parenthèse ouverte).
2. Si WebKitGTK est installé (sous Xvfb) : chaque chaîne réécrite, y compris
   600 chaînes aléatoires, est appliquée par webkit_settings_set_user_agent
   sans assertion CRITICAL, et une chaîne vide rend l'agent par défaut.
"""
import importlib.util
from pathlib import Path
import os
import random
import sys

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("winlux_webview", HERE / "winlux-webview.py")
webview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(webview)
header = webview.user_agent_header
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


EDGE = ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0")
cases = {
    EDGE: EDGE,
    "": "",
    " Teams/24335.208.3315.1951/49": "Teams/24335.208.3315.1951.49",
    EDGE + " Teams/1/49 (Windows NT 10.0; ms-teams/2)": EDGE + " Teams/1.49 (Windows NT 10.0; ms-teams/2)",
    "x;y": "x-y",
    "Teams/1,": "Teams/1",
    "été/1": "t/1",
    "a (b (c) d": "a (b (c) d)",
    "(seul) a/1": "a/1",
    "a\tb": "a b",
}
for raw, expected in cases.items():
    got = header(raw)
    check(got == expected, f"user_agent_header({raw!r}) = {got!r}, attendu {expected!r}")
    check(header(got) == got, f"réécriture non stable pour {raw!r}")

rng = random.Random(2605)
alphabet = "aZ09/ ()\\;,é\t\"[]{}=?@:" + "".join(chr(c) for c in range(0x21, 0x7f))
fuzz = ["".join(rng.choice(alphabet) for _ in range(rng.randint(1, 60))) for _ in range(600)]
for raw in fuzz:
    check(header(header(raw)) == header(raw), f"réécriture non stable pour {raw!r}")

WebKit2 = None
try:
    if not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
        raise ImportError("pas d'affichage")
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("WebKit2", "4.1")
    from gi.repository import Gtk, WebKit2  # noqa: F401 - Gtk initialise l'affichage
except (ImportError, ValueError):
    WebKit2 = None
if WebKit2 is None:
    print("WebKitGTK ou affichage absent : partie moteur non vérifiée")
else:
    settings = WebKit2.Settings()
    default = settings.get_user_agent()
    for raw in list(cases) + fuzz:
        applied = header(raw)
        settings.set_user_agent(default)
        settings.set_user_agent(applied or None)
        want = applied or default
        check(settings.get_user_agent() == want, f"WebKit a refusé {applied!r} (depuis {raw!r})")

if failures:
    sys.exit(1)
print("Agent utilisateur : toute chaîne put_UserAgent est appliquée par WebKit (#2611)")
