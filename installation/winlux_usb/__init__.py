"""Créateur de clé USB d'installation Winlux.

Écrit l'image ISO hybride de Winlux (amorçable en UEFI et en BIOS) sur une
clé USB, depuis Windows ou depuis Linux/Winlux. La clé démarre ensuite le
système Live et son installateur (« Installer Winlux sur le disque »).
"""

APP_ID = "winlux-usb-creator"
REPO = "Univers4craft/winlux"


def version():
    """Version de l'outil : celle de Winlux, gravée au moment de la construction."""
    try:
        from . import _version  # généré par la CI (build-deb.sh, PyInstaller)
        return _version.VERSION
    except ImportError:
        pass
    from pathlib import Path
    for candidate in (Path(__file__).resolve().parents[2] / "VERSION",):
        try:
            return candidate.read_text(encoding="utf-8").strip()
        except OSError:
            continue
    return "dev"
