"""Adaptation de l'interface à l'écran (résolution et densité de pixels).

compute_scale() choisit un facteur d'échelle proche de ce que Windows propose
(100 %, 125 %, 150 %…) à partir de la densité réelle de l'écran quand l'EDID la
donne, sinon de la hauteur en pixels. Le facteur est ensuite réduit si l'écran
est trop petit pour afficher l'interface (1024x600, 1280x720…).
"""
import math
import os
import re

BASE_WIDTH = 1100
BASE_HEIGHT = 680
MIN_SCALE = 0.7
MAX_SCALE = 4.0
REFERENCE_DPI = 112.0


def _round_quarter(value):
    return round(value * 4) / 4


def physical_dpi(width_px, width_mm):
    if not width_mm or width_mm < 100:
        return None
    dpi = width_px / (width_mm / 25.4)
    # Sans EDID (VM, Xvfb, certains pilotes), X invente une taille à 96 ppp exactement.
    if abs(dpi - 96) < 1.5:
        return None
    return dpi if 60 <= dpi <= 500 else None


def scale_override(environ=None, cmdline_path="/proc/cmdline"):
    environ = os.environ if environ is None else environ
    raw = environ.get("WINLUX_UI_SCALE", "")
    if not raw:
        try:
            with open(cmdline_path) as f:
                m = re.search(r"(?:^|\s)winlux\.scale=([0-9.]+)", f.read())
                raw = m.group(1) if m else ""
        except OSError:
            raw = ""
    try:
        value = float(raw)
    except ValueError:
        return None
    return min(MAX_SCALE, max(0.5, value)) if value > 0 else None


def compute_scale(width_px, height_px, width_mm=0, height_mm=0, override=None):
    """Facteur d'échelle global pour un écran de width_px x height_px pixels physiques."""
    if override:
        return override
    if width_px <= 0 or height_px <= 0:
        return 1.0
    dpi = physical_dpi(width_px, width_mm) if height_mm and height_mm >= 60 else None
    if dpi:
        wanted = _round_quarter(dpi / REFERENCE_DPI)
    else:
        wanted = _round_quarter(height_px / 900)
    wanted = min(MAX_SCALE, max(1.0, wanted))
    fit = min(width_px / BASE_WIDTH, height_px / BASE_HEIGHT)
    if fit < wanted:
        wanted = max(MIN_SCALE, math.floor(fit * 20) / 20)
        if fit >= 1.0:
            wanted = max(1.0, math.floor(fit * 4) / 4)
    return wanted


_PX = re.compile(rb"(?<![\w.#-])(\d+(?:\.\d+)?)px")


def scale_css(css, factor):
    """Multiplie toutes les longueurs en px d'une feuille de style GTK."""
    if abs(factor - 1.0) < 0.01:
        return css

    def repl(m):
        value = float(m.group(1))
        scaled = value * factor
        if value > 0:
            scaled = max(1, round(scaled))
        return str(int(scaled)).encode() + b"px"
    return _PX.sub(repl, css)


def is_compact(height_px_logical):
    return height_px_logical < 760


def monitor_info(Gdk, window=None):
    """(géométrie logique, facteur GDK, largeur mm, hauteur mm) du moniteur principal."""
    display = Gdk.Display.get_default()
    monitor = None
    if window is not None and window.get_window() is not None:
        monitor = display.get_monitor_at_window(window.get_window())
    if monitor is None:
        monitor = display.get_primary_monitor() or display.get_monitor(0)
    if monitor is None:
        screen = Gdk.Screen.get_default()
        rect = Gdk.Rectangle()
        rect.x, rect.y, rect.width, rect.height = 0, 0, screen.get_width(), screen.get_height()
        return rect, 1, 0, 0
    return (monitor.get_geometry(), monitor.get_scale_factor(),
            monitor.get_width_mm(), monitor.get_height_mm())
