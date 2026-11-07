#!/usr/bin/python3
"""Touches de volume du clavier (#620) : plus, moins, sourdine, micro.

Openbox lie XF86AudioRaiseVolume/LowerVolume/Mute/MicMute à cette commande.
Comme sous Windows : pas de 2 %, plafond 100 %, monter le volume rétablit le
son coupé, et un indicateur (haut-parleur et barre) s'affiche en bas de
l'écran. Même ordre de mixeurs que le panneau Son (winlux-popup) : PipeWire
ou PulseAudio par pactl (en locale C, sinon « 0,40 »), puis wpctl, puis ALSA.

  winlux-volume key up|down|mute|mic-mute    touche du clavier
  winlux-volume get                          état (JSON)
WINLUX_VOLUME_NO_OSD=1 coupe l'indicateur ; WINLUX_VOLUME_RUN déplace /run/winlux.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

RUN = Path(os.environ.get("WINLUX_VOLUME_RUN", "/run/winlux"))
STEP = 2


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    "Volume": ("Volume", "Volumen", "Lautstärke"),
    "Son coupé": ("Muted", "Silenciado", "Stumm"),
    "Micro coupé": ("Microphone muted", "Micrófono silenciado", "Mikrofon stumm"),
    "Micro activé": ("Microphone on", "Micrófono activado", "Mikrofon an"),
    "Volume de sortie, en pourcentage": ("Output volume, as a percentage", "Volumen de salida, en porcentaje",
                                         "Ausgabelautstärke in Prozent"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def _run(args, timeout=2):
    # pactl en locale C : en français son JSON écrit « 0,00 » (JSON invalide).
    env = dict(os.environ, LC_ALL="C", LANG="C")
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout, env=env)
        return result.stdout if result.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def _percent(volume):
    levels = []
    for channel in (volume or {}).values():
        try: levels.append(int(str(channel.get("value_percent", "0")).strip().rstrip("%")))
        except (ValueError, AttributeError): pass
    return max(levels or [0])


def state():
    """{"backend", "percent", "muted"} de la sortie par défaut ; backend None sans sortie."""
    if shutil.which("pactl"):
        default = (_run(["pactl", "get-default-sink"]) or "").strip()
        sinks = _run(["pactl", "-f", "json", "list", "sinks"])
        if default and default != "auto_null" and sinks:
            try:
                for sink in json.loads(sinks):
                    if sink.get("name") == default:
                        return {"backend": "pactl", "percent": min(100, _percent(sink.get("volume"))), "muted": bool(sink.get("mute"))}
            except (ValueError, TypeError, AttributeError):
                pass
    if shutil.which("wpctl"):
        output = _run(["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"])
        found = re.search(r"Volume:\s*([0-9.]+)", output or "")
        if found:
            return {"backend": "wpctl", "percent": min(100, round(float(found[1]) * 100)), "muted": "MUTED" in output}
    if shutil.which("amixer"):
        output = _run(["amixer", "get", "Master"])
        found = re.findall(r"\[(\d+)%\]", output or "")
        if found:
            return {"backend": "amixer", "percent": int(found[-1]), "muted": "[off]" in output}
    return {"backend": None, "percent": 0, "muted": False}


def stepped(percent, up):
    """Pas de 2 % calé sur les multiples de 2, comme Windows (41 → 42 ou 40)."""
    value = (percent // STEP + 1) * STEP if up else (-(-percent // STEP) - 1) * STEP
    return max(0, min(100, value))


def set_volume(backend, percent):
    if backend == "pactl":
        _run(["pactl", "set-sink-volume", "@DEFAULT_SINK@", f"{percent}%"])
    elif backend == "wpctl":
        _run(["wpctl", "set-volume", "-l", "1", "@DEFAULT_AUDIO_SINK@", f"{percent}%"])
    elif backend == "amixer":
        _run(["amixer", "set", "Master", f"{percent}%"])


def set_mute(backend, muted):
    if backend == "pactl":
        _run(["pactl", "set-sink-mute", "@DEFAULT_SINK@", "1" if muted else "0"])
    elif backend == "wpctl":
        _run(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "1" if muted else "0"])
    elif backend == "amixer":
        _run(["amixer", "set", "Master", "mute" if muted else "unmute"])


def mic_toggle():
    """Sourdine du micro par défaut ; True si coupé, None sans micro."""
    if shutil.which("pactl") and _run(["pactl", "set-source-mute", "@DEFAULT_SOURCE@", "toggle"]) is not None:
        output = _run(["pactl", "get-source-mute", "@DEFAULT_SOURCE@"]) or ""
        return "yes" in output
    if shutil.which("wpctl") and _run(["wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]) is not None:
        return "MUTED" in (_run(["wpctl", "get-volume", "@DEFAULT_AUDIO_SOURCE@"]) or "")
    if shutil.which("amixer"):
        output = _run(["amixer", "set", "Capture", "toggle"])
        if output is not None: return "[off]" in output
    return None


def key(action):
    if action == "mic-mute":
        muted = mic_toggle()
        if muted is not None: osd(None, muted, mic=True)
        return {"ok": muted is not None, "mic_muted": muted}
    current = state()
    backend = current["backend"]
    if backend is None: return {"ok": False, "error": "no-output"}
    if action == "mute":
        set_mute(backend, not current["muted"])
    else:
        up = action == "up"
        set_volume(backend, stepped(current["percent"], up))
        # Monter le volume rétablit le son, comme Windows ; le baisser non.
        if up and current["muted"]: set_mute(backend, False)
    after = state()
    osd(after["percent"], after["muted"])
    return dict(after, ok=True)


# ── Indicateur à l'écran (même allure que celui de la luminosité) ───────────

def icon_name(percent, muted, mic=False):
    if mic: return "microphone-sensitivity-muted-symbolic" if muted else "audio-input-microphone-symbolic"
    if muted or not percent: return "audio-volume-muted-symbolic"
    return "audio-volume-low-symbolic" if percent < 34 else "audio-volume-medium-symbolic" if percent < 67 else "audio-volume-high-symbolic"


def osd(percent, muted, mic=False, seconds=1.5):
    """Dans un processus à part : la touche rend la main tout de suite. Un
    nouvel appui remplace l'ancien indicateur."""
    if not os.environ.get("DISPLAY") or os.environ.get("WINLUX_VOLUME_NO_OSD") == "1": return
    marker = RUN / f"volume-osd-{os.getuid()}"
    stamp = f"{percent} {muted} {time.time()}\n"
    try:
        RUN.mkdir(parents=True, exist_ok=True)
        marker.write_text(stamp)
    except OSError:
        pass
    if os.fork() != 0: return
    try:
        os.setsid()
        show_osd(percent, muted, mic, seconds, marker, stamp)
    finally:
        os._exit(0)


def show_osd(percent, muted, mic, seconds, marker, stamp):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.set_name("winlux-volume-osd")
    window.set_title(_t("Volume"))
    window.set_keep_above(True)
    window.set_accept_focus(False)
    box = Gtk.Box(spacing=12)
    box.set_border_width(14)
    box.pack_start(Gtk.Image.new_from_icon_name(icon_name(percent, muted, mic), Gtk.IconSize.LARGE_TOOLBAR), False, False, 0)
    if mic:
        box.pack_start(Gtk.Label(label=_t("Micro coupé") if muted else _t("Micro activé")), True, True, 0)
    else:
        bar = Gtk.LevelBar(min_value=0, max_value=100)
        bar.set_value(0 if muted else percent)
        bar.set_size_request(180, 6)
        bar.set_valign(Gtk.Align.CENTER)
        box.pack_start(bar, True, True, 0)
        box.pack_start(Gtk.Label(label=_t("Son coupé") if muted else f"{percent}"), False, False, 0)
        window.get_accessible().set_name(_t("Volume de sortie, en pourcentage"))
    window.add(box)
    css = Gtk.CssProvider()
    css.load_from_data(b"#winlux-volume-osd { background: #2b2a33; color: #f3f0fa; border-radius: 10px; }"
                       b"#winlux-volume-osd levelbar block.filled { background: #b69cff; }")
    Gtk.StyleContext.add_provider_for_screen(window.get_screen(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
    window.show_all()
    screen = window.get_screen()
    width, height = window.get_size()
    window.move((screen.get_width() - width) // 2, screen.get_height() - height - 96)

    def done():
        Gtk.main_quit()
        return False

    def check():
        try: current = marker.read_text()
        except OSError: current = stamp
        return done() if current != stamp else True
    GLib.timeout_add(int(seconds * 1000), done)
    GLib.timeout_add(100, check)
    Gtk.main()


def main(argv):
    args = argv[1:]
    if args == ["get"]:
        answer = state()
    elif len(args) == 2 and args[0] == "key" and args[1] in ("up", "down", "mute", "mic-mute"):
        answer = key(args[1])
    else:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    print(json.dumps(answer, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
