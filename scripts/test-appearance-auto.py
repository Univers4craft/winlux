#!/usr/bin/python3
"""Thème automatique, fond en diaporama ou selon l'heure, thèmes partageables (#281)."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import zipfile

HERE = Path(__file__).resolve().parent
AMBIANCE = HERE / "winlux-ambiance.py"
PERSONALIZE = HERE / "winlux-personalize.py"

with tempfile.TemporaryDirectory() as folder:
    root = Path(folder)
    zones = root / "zoneinfo"
    zones.mkdir()
    (zones / "zone1970.tab").write_text("FR,MC\t+4852+00220\tEurope/Paris\n")
    walls = root / "share/wallpapers"
    walls.mkdir(parents=True)
    for name in ("winlux-prisme-aube.png", "winlux-prisme-nuit.png"):
        (walls / name).write_bytes(b"png")
    photos = root / "photos"
    photos.mkdir()
    for name in ("b.jpg", "a.png", "c.JPEG", "notes.txt", ".cache.png"):
        (photos / name).write_bytes(b"x")
    log = root / "personalize.log"
    fake = root / "personalize.sh"
    fake.write_text(f'#!/bin/sh\necho "$1 $2" >> "{log}"\n')
    fake.chmod(0o755)
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), WINLUX_ZONEINFO=str(zones), TZ="Europe/Paris",
               WINLUX_WALLPAPERS=str(root / "share"), WINLUX_PERSONALIZE=str(fake), WINLUX_AMBIANCE_FAKE=str(root / "gamma"))

    def tool(*args, now="2026-11-02T12:00", check=True):
        result = subprocess.run([sys.executable, "-I", str(AMBIANCE), *args], env=dict(env, WINLUX_AMBIANCE_NOW=now),
                                capture_output=True, text=True, timeout=20)
        if check: assert result.returncode == 0, (args, result.stderr)
        return result

    def calls():
        lines = log.read_text().splitlines() if log.exists() else []
        log.unlink(missing_ok=True)
        return lines

    # Validation des nouvelles clés ; les clés internes sont refusées.
    assert tool("set", "THEME_AUTO", "peut-être", check=False).returncode == 2
    assert tool("set", "SLIDE_MINUTES", "7", check=False).returncode == 2
    assert tool("set", "SLIDE_FOLDER", str(root / "absent"), check=False).returncode == 2
    assert tool("set", "THEME_STAMP", "x", check=False).returncode == 2
    # Mode automatique : clair à midi, sombre le soir, une seule fois par plage.
    tool("set", "THEME_AUTO", "on")
    assert calls() == ["theme light"]
    status = dict(line.split("=", 1) for line in tool("status").stdout.split())
    assert status["theme_auto"] == "on" and status["wallpaper"] == "fixed"
    module = runpy.run_path(str(AMBIANCE), run_name="ambiance_test")
    os.environ.update(WINLUX_ZONEINFO=str(zones), TZ="Europe/Paris", WINLUX_WALLPAPERS=str(root / "share"),
                      WINLUX_PERSONALIZE=str(fake), XDG_CONFIG_HOME=str(root / "config"))
    import time
    time.tzset()
    import datetime as dt
    values = module["load"]()
    assert module["theme_due"](values, dt.datetime(2026, 11, 2, 13, 0)) is None, "même journée : rien"
    assert module["theme_due"](values, dt.datetime(2026, 11, 2, 19, 0)) == "dark"
    assert module["theme_due"](values, dt.datetime(2026, 11, 3, 2, 0)) is None, "même nuit : rien"
    assert module["theme_due"](values, dt.datetime(2026, 11, 3, 9, 0)) == "light"
    assert module["theme_due"](dict(values, THEME_AUTO="off"), dt.datetime(2026, 11, 3, 20, 0)) is None
    # Fond Aube le jour, Nuit le soir.
    day = dict(module["DEFAULTS"], WALLPAPER="daynight")
    assert module["daynight_due"](day, dt.datetime(2026, 11, 2, 12, 0)).endswith("winlux-prisme-aube.png")
    assert module["daynight_due"](day, dt.datetime(2026, 11, 2, 15, 0)) is None
    assert module["daynight_due"](day, dt.datetime(2026, 11, 2, 21, 0)).endswith("winlux-prisme-nuit.png")
    # Diaporama : images seulement, dans l'ordre, à l'intervalle choisi, en boucle.
    assert [Path(p).name for p in module["slideshow_images"](photos)] == ["a.png", "b.jpg", "c.JPEG"]
    show = dict(module["DEFAULTS"], WALLPAPER="slideshow", SLIDE_FOLDER=str(photos), SLIDE_MINUTES="10")
    picks = [module["slideshow_due"](show, t) for t in (1000, 1300, 1600, 2200, 2800)]
    assert [Path(p).name if p else None for p in picks] == ["a.png", None, "b.jpg", "c.JPEG", "a.png"], picks
    show.update(SLIDE_SHUFFLE="on", SLIDE_INDEX="0")
    for t in range(10):
        before = show["SLIDE_INDEX"]
        module["slideshow_due"](show, 10_000 + t * 1000, force=True)
        assert show["SLIDE_INDEX"] != before, "jamais deux fois la même image d'affilée"
    # Commandes : diaporama appliqué tout de suite, « image suivante ».
    tool("set", "THEME_AUTO", "off")
    tool("set", "WALLPAPER", "slideshow")
    tool("set", "SLIDE_FOLDER", str(photos))
    assert calls() == [f"wallpaper {photos}/a.png"]
    tool("next")
    assert calls() == [f"wallpaper {photos}/b.jpg"]
    tool("set", "WALLPAPER", "daynight", now="2026-11-02T22:00")
    assert calls() == [f"wallpaper {walls}/winlux-prisme-nuit.png"]

# Thèmes partageables : export, relecture, fichiers refusés.
personalize = runpy.run_path(str(PERSONALIZE), run_name="personalize_test")
with tempfile.TemporaryDirectory() as folder:
    root = Path(folder)
    picture = root / "fond.png"
    picture.write_bytes(b"\x89PNG fond")
    theme = root / "Mon thème.wlxtheme"
    settings = personalize["export_theme"]({"theme": "light", "accent": "#0F6CBD", "panel-size": "48",
                                            "wallpaper": str(picture), "extra": "ignoré"}, theme, sounds=False)
    assert settings["wallpaper"] == "wallpaper.png" and "extra" not in settings
    read, image, suffix = personalize["read_theme"](theme)
    assert read == {"format": "winlux-theme-1", "theme": "light", "accent": "#0f6cbd", "panel-size": "48",
                    "sounds": False, "name": "Mon thème"}, read
    assert image == b"\x89PNG fond" and suffix == ".png"

    def bad(members, label):
        path = root / f"{label}.wlxtheme"
        with zipfile.ZipFile(path, "w") as archive:
            for name, data in members.items(): archive.writestr(name, data)
        try:
            personalize["read_theme"](path)
        except ValueError:
            return
        raise AssertionError(label)
    bad({"theme.json": json.dumps({"format": "autre"})}, "format")
    bad({"theme.json": "{"}, "json")
    bad({"theme.json": json.dumps({"format": "winlux-theme-1"}), "../evil.sh": "x"}, "chemin")
    bad({"wallpaper.png": "x"}, "sans-json")
    (root / "pas-un-zip.wlxtheme").write_text("bonjour")
    try:
        personalize["read_theme"](root / "pas-un-zip.wlxtheme")
        raise AssertionError("zip")
    except ValueError:
        pass
    # Valeurs invalides ignorées, pas appliquées.
    path = root / "valeurs.wlxtheme"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("theme.json", json.dumps({"format": "winlux-theme-1", "theme": "rose", "accent": "rouge",
                                                   "panel-size": "999", "name": "<script>"}))
    read, image, _ = personalize["read_theme"](path)
    assert read == {"format": "winlux-theme-1", "name": "script"} and image is None, read
# Thème appliqué à chaud (VM : Paramètres et les fenêtres ouvertes gardaient
# l'ancien thème ; /etc/winlux/preferences.conf restait à THEME=dark).
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    home, tools = root / "home", root / "bin"
    tools.mkdir()
    (home / ".config/gtk-3.0").mkdir(parents=True)
    (home / ".config/gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name=Adwaita\ngtk-application-prefer-dark-theme=1\n")
    (home / ".config/gtk-3.0/gtk.css").write_text(".tasklist {padding: 0;}\n/* Winlux appearance */\nwindow {color: red;}\n/* End Winlux appearance */\n")
    calls = root / "calls.log"
    for tool in ("xfconf-query", "winlux-control", "openbox"):
        (tools / tool).write_text(f'#!/bin/sh\necho "{tool} $*" >> "{calls}"\n')
        (tools / tool).chmod(0o755)
    env = {**os.environ, "HOME": str(home), "PATH": f"{tools}:{os.environ['PATH']}"}

    def apply(*args):
        subprocess.run([sys.executable, "-B", str(PERSONALIZE), *args], env=env, check=True, stdout=subprocess.DEVNULL)
        ini = (home / ".config/gtk-3.0/settings.ini").read_text()
        return next(line.split("=", 1)[1].strip() for line in ini.splitlines() if line.startswith("gtk-theme-name"))

    light = apply("theme", "light")
    log = calls.read_text()
    assert light.startswith("Winlux-light-"), light
    assert f"-p /Net/ThemeName -s {light}" in log, log
    assert "winlux-control theme-light" in log, log
    ini = (home / ".config/gtk-3.0/settings.ini").read_text()
    assert "gtk-application-prefer-dark-theme = 0" in ini, ini
    css = (home / ".local/share/themes" / light / "gtk-3.0/gtk.css").read_text()
    assert "gtk-contained.css" in css and "#f3f5fa" in css, css
    style = (home / ".config/gtk-3.0/gtk.css").read_text()
    assert ".tasklist" in style and "Winlux appearance" not in style, style
    again = apply("accent", "#2e8b57")
    assert again.startswith("Winlux-light-") and again != light, (light, again)  # nouveau nom : rechargé partout
    dark = apply("theme", "dark")
    assert dark.startswith("Winlux-dark-") and "winlux-control theme-dark" in calls.read_text()
    assert "gtk-contained-dark.css" in (home / ".local/share/themes" / dark / "gtk-3.0/gtk.css").read_text()

for key, row in personalize["_TR"].items():
    assert len(row) == 3 and all(row), key

# Paramètres : mode automatique, diaporama et thèmes, traduits.
pages = (HERE / "winlux-settings-pages.py").read_text()
for text in ("'Automatique : clair le jour, sombre le soir':", "'Diaporama':", "'Enregistrer le thème…':", "def theme_files(self):"):
    assert text in pages, text
print("thème automatique, diaporama et thèmes : OK")
