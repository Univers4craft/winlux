#!/usr/bin/python3
"""Real device state for the native panel; never invent battery/network levels."""
from pathlib import Path
import html
import re
import subprocess
import sys

kind = sys.argv[1]
directory = Path("/run/winlux/tray")
directory.mkdir(parents=True, exist_ok=True)
color = "#e7ddf5"
if kind == "battery":
    batteries = list(Path("/sys/class/power_supply").glob("BAT*/capacity"))
    if batteries:
        level = max(0, min(100, int(batteries[0].read_text().strip())))
        status = (batteries[0].parent / "status").read_text().strip()
        body = f'<rect x="3" y="7" width="18" height="11" rx="2" fill="none" stroke="{color}" stroke-width="1.5"/><path d="M22 10v5" stroke="{color}" stroke-width="2"/><rect x="5" y="9" width="{14 * level / 100:.1f}" height="7" fill="{color}"/>'
        tooltip, identity = f"Batterie : {level}% — {status}", f"battery-{level}"
    else:
        body = f'<path d="M8 3v5m8-5v5M6 8h12v4a6 6 0 0 1-6 6v4m0-4a6 6 0 0 1-6-6V8" fill="none" stroke="{color}" stroke-width="1.8"/>'
        tooltip, identity = "Alimentation secteur — aucune batterie exposée par la machine", "power-ac"
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
    tooltip = "Audio indisponible" if level is None else "Son coupé" if muted else f"Volume : {level}%"
    identity, page = f"audio-{wave}", 3
elif kind == "network":
    active = [p for p in Path("/sys/class/net").iterdir() if p.name != "lo" and (p / "operstate").read_text().strip() == "up"]
    wireless = any((path / "wireless").exists() for path in active)
    if wireless:
        body = f'<path d="M2 9q10-10 20 0M5 13q7-7 14 0M9 17q3-3 6 0" fill="none" stroke="{color}" stroke-width="2"/><circle cx="12" cy="21" r="1.3" fill="{color}"/>'
    else:
        body = f'<rect x="3" y="4" width="18" height="12" rx="2" fill="none" stroke="{color}" stroke-width="1.6"/><path d="M12 16v5m-5 0h10" stroke="{color}" stroke-width="1.6"/>'
    if not active: body += '<path d="m4 4 16 16" stroke="#cf8ea9" stroke-width="2"/>'
    tooltip = ("Wi-Fi : " if wireless else "Ethernet : ") + ", ".join(path.name for path in active) if active else "Réseau déconnecté"
    identity, page = f"network-{int(bool(active))}-{int(wireless)}", 2
elif kind == "notifications":
    body = f'<path d="M5 17h14l-2-4V9a5 5 0 0 0-10 0v4zm5 3h4" fill="none" stroke="{color}" stroke-width="2"/>'
    tooltip, identity, page = "Centre de notifications", "notifications", 0
else:
    raise SystemExit(2)
icon = directory / (identity + ".svg")
if not icon.exists(): icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{body}</svg>')
target = "notifications" if kind == "notifications" else "quick"
command = "/usr/bin/winlux-popup volume" if kind=="audio" else "/usr/bin/winlux-popup power" if kind=="battery" else f"/usr/bin/winlux-system-tools {target}"
print(f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>{command}</click>")
