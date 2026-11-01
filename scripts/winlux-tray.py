#!/usr/bin/python3
"""Real device state for the native panel; never invent battery/network levels."""
from pathlib import Path
import html
import re
import subprocess
import sys
import os


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Alimentation secteur — aucune batterie exposée par la machine': ('Plugged in — this computer reports no battery', 'Conectado a la corriente — el equipo no informa de ninguna batería', 'Netzbetrieb — der Computer meldet keinen Akku'),
    'Audio indisponible': ('Audio unavailable', 'Audio no disponible', 'Audio nicht verfügbar'),
    'Son coupé': ('Sound muted', 'Sonido silenciado', 'Ton aus'),
    'Wi-Fi : ': ('Wi-Fi: ', 'Wi-Fi: ', 'WLAN: '),
    'Ethernet : ': ('Ethernet: ', 'Ethernet: ', 'Ethernet: '),
    'Réseau déconnecté': ('Network disconnected', 'Red desconectada', 'Netzwerk getrennt'),
    'Centre de notifications': ('Notification center', 'Centro de notificaciones', 'Benachrichtigungscenter'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


kind = sys.argv[1]
directory = Path("/run/winlux/tray")
directory.mkdir(parents=True, exist_ok=True)
color = "#e7ddf5"
if kind == "battery":
    batteries = list(Path("/sys/class/power_supply").glob("BAT*/capacity"))
    level = None
    if batteries:
        # Une batterie absente ou en défaut expose parfois un fichier vide ou illisible.
        try:
            level = max(0, min(100, int(batteries[0].read_text().strip())))
            status = (batteries[0].parent / "status").read_text().strip()
        except (OSError, ValueError):
            level = None
    if level is not None:
        body = f'<rect x="3" y="7" width="18" height="11" rx="2" fill="none" stroke="{color}" stroke-width="1.5"/><path d="M22 10v5" stroke="{color}" stroke-width="2"/><rect x="5" y="9" width="{14 * level / 100:.1f}" height="7" fill="{color}"/>'
        tooltip, identity = f"Batterie : {level}% — {status}", f"battery-{level}"
    else:
        body = f'<path d="M8 3v5m8-5v5M6 8h12v4a6 6 0 0 1-6 6v4m0-4a6 6 0 0 1-6-6V8" fill="none" stroke="{color}" stroke-width="1.8"/>'
        tooltip, identity = _t("Alimentation secteur — aucune batterie exposée par la machine"), "power-ac"
    page = 3
elif kind == "audio":
    try:
        result = subprocess.run(["amixer", "get", "Master"], capture_output=True, text=True, timeout=2)
        values = re.findall(r"\[(\d+)%\]", result.stdout)
        level = int(values[-1]) if values else None
        muted = "[off]" in result.stdout
    except (OSError, subprocess.TimeoutExpired): level, muted = None, False
    wave = 0 if level is None or muted or level == 0 else 1 if level < 40 else 2 if level < 75 else 3
    body = f'<path d="M3 9h4l5-5v16l-5-5H3z" fill="{color}"/>'
    if not wave: body += f'<path d="m16 9 6 6m0-6-6 6" stroke="{color}" stroke-width="1.7"/>'
    for n in range(wave): body += f'<path d="M{15 + n * 3} {9 - n * 2}q{3 + n} {3 + n * 2} 0 {6 + n * 4}" fill="none" stroke="{color}" stroke-width="1.4"/>'
    tooltip = _t("Audio indisponible") if level is None else _t("Son coupé") if muted else f"Volume : {level}%"
    identity, page = f"audio-{wave}", 3
elif kind == "network":
    try:
        routes = subprocess.check_output(["ip", "-4", "route", "show", "default"], text=True, timeout=2).splitlines()
        fields = routes[0].split() if routes else []
        interface = fields[fields.index("dev") + 1] if "dev" in fields else ""
    except (OSError, subprocess.SubprocessError, IndexError): interface = ""
    active = [Path("/sys/class/net") / interface] if interface and "/" not in interface else []
    wireless = bool(active and (active[0] / "wireless").exists())
    if wireless:
        body = f'<path d="M2 9q10-10 20 0M5 13q7-7 14 0M9 17q3-3 6 0" fill="none" stroke="{color}" stroke-width="2"/><circle cx="12" cy="21" r="1.3" fill="{color}"/>'
    else:
        body = f'<rect x="3" y="4" width="18" height="12" rx="2" fill="none" stroke="{color}" stroke-width="1.6"/><path d="M12 16v5m-5 0h10" stroke="{color}" stroke-width="1.6"/>'
    if not active: body += '<path d="m4 4 16 16" stroke="#cf8ea9" stroke-width="2"/>'
    tooltip = (_t("Wi-Fi : ") if wireless else _t("Ethernet : ")) + ", ".join(path.name for path in active) if active else _t("Réseau déconnecté")
    identity, page = f"network-{int(bool(active))}-{int(wireless)}", 2
elif kind == "notifications":
    body = f'<path d="M5 17h14l-2-4V9a5 5 0 0 0-10 0v4zm5 3h4" fill="none" stroke="{color}" stroke-width="2"/>'
    tooltip, identity, page = _t("Centre de notifications"), "notifications", 0
else:
    raise SystemExit(2)
icon = directory / (identity + ".svg")
if not icon.exists(): icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{body}</svg>')
target = "notifications" if kind == "notifications" else "quick"
command = "/usr/bin/winlux-popup volume" if kind=="audio" else "/usr/bin/winlux-popup power" if kind=="battery" else f"/usr/bin/winlux-system-tools {target}"
print(f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>{command}</click>")
