#!/usr/bin/python3
"""Application Jeux : détection des jeux (GOG, itch.io, dossiers, programmes
installés), temps de jeu, raccourcis, réglages et traductions.

    python3 scripts/test-games.py          # bibliothèque, sans affichage
    xvfb-run python3 scripts/test-games.py --gtk [capture.png]   # + fenêtre
"""
import gzip
import importlib.util
import json
import os
from pathlib import Path
import re
import struct
import sys
import tempfile
import time

project = Path(__file__).resolve().parent.parent
work = Path(tempfile.mkdtemp(prefix="winlux-jeux-test-"))
home = work / "moi"
home.mkdir()
for variable in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "WINLUX_DRIVE_C", "WINLUX_HOME"):
    os.environ.pop(variable, None)
os.environ["HOME"] = str(home)
os.environ["WINLUX_LANG"] = "fr"

source = project / "integration/native/jeux/winlux-games.py"
spec = importlib.util.spec_from_file_location("winlux_games", source)
games = importlib.util.module_from_spec(spec)
spec.loader.exec_module(games)


def pe(path, imports=(), wide=True, dll=False):
    """Petit .exe PE valide important les DLL données (jamais exécuté)."""
    optional_size = 240 if wide else 224
    directories = 112 if wide else 96
    header = bytearray(0x200)
    header[0:2] = b"MZ"
    struct.pack_into("<I", header, 60, 64)
    header[64:68] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", header, 68, 0x8664 if wide else 0x14C, 1, 0, 0, 0, optional_size,
                     0x2022 if dll else 0x22)
    optional = 88
    struct.pack_into("<H", header, optional, 0x20B if wide else 0x10B)
    struct.pack_into("<II", header, optional + directories + 8, 0x1000, 20 * (len(imports) + 1))
    section = optional + optional_size
    header[section:section + 8] = b".idata\0\0"
    struct.pack_into("<IIII", header, section + 8, 0x200, 0x1000, 0x200, 0x200)
    data = bytearray(0x200)
    names = 20 * (len(imports) + 1)
    for index, name in enumerate(imports):
        struct.pack_into("<I", data, index * 20 + 12, 0x1000 + names)
        encoded = name.encode() + b"\0"
        data[names:names + len(encoded)] = encoded
        names += len(encoded)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(header) + bytes(data))
    return path


# ── Lecture des .exe ──
probe = pe(work / "probe.exe", ["KERNEL32.dll", "d3d11.dll", "XInput1_4.dll"])
assert games.pe_info(probe) == (True, ["kernel32.dll", "d3d11.dll", "xinput1_4.dll"]), games.pe_info(probe)
assert games.heavy(games.pe_info(probe)[1])
assert not games.heavy(["kernel32.dll", "user32.dll"])
assert games.pe_info(pe(work / "lib.exe", ["d3d9.dll"], dll=True)) is None, "une DLL n’est pas un jeu"
assert games.pe_info(pe(work / "old.exe", ["d3d9.dll"], wide=False)) == (False, ["d3d9.dll"])
(work / "texte.exe").write_text("pas un programme")
assert games.pe_info(work / "texte.exe") is None
assert games.is_tool("unins000.exe") and games.is_tool("UnityCrashHandler64.exe") and not games.is_tool("Hollow.exe")

# ── Jeux installés de toutes les sources ──
super_jeu = pe(home / "Games/super_jeu/bin/superjeu.exe", ["d3d11.dll", "kernel32.dll"])
pe(home / "Games/super_jeu/unins000.exe", ["d3d11.dll"])
pe(home / "Games/super_jeu/launcher.exe", ["kernel32.dll"])
pe(home / "Games/super_jeu/redist/vc_redist.x64.exe", ["d3d11.dll"])
vieux = pe(home / "Jeux/Vieux jeu/vieux.exe", ["d3d9.dll"], wide=False)
witcher = pe(home / "GOG Games/The Witcher/bin/x64/witcher.exe", ["kernel32.dll"])
(home / "GOG Games/The Witcher/goggame-1207658924.info").write_text(json.dumps({
    "gameId": "1207658924", "name": "The Witcher: Enhanced Edition",
    "playTasks": [{"category": "launcher", "path": "launcher.exe", "type": "FileTask"},
                  {"isPrimary": True, "path": "bin\\x64\\witcher.exe", "type": "FileTask"}]}))
tiny = pe(home / "AppData/Roaming/itch/apps/tiny-game/TinyGame.exe", ["opengl32.dll"])
(tiny.parent / ".itch").mkdir()
with gzip.open(tiny.parent / ".itch/receipt.json.gz", "wt") as stream:
    json.dump({"game": {"title": "Tiny Game"}}, stream)
racer = pe(home / "Program Files/Racer/racer.exe", ["xinput1_4.dll", "dxgi.dll"])
pe(home / "Program Files/Editeur/editeur.exe", ["kernel32.dll", "user32.dll"])
extra = work / "Disque D/Mes jeux"
other = pe(extra / "Plateformes/plateformes.exe", ["vulkan-1.dll"])
hidden = pe(extra / "Ennuyeux/ennuyeux.exe", ["d3d12.dll"])
manual = pe(work / "Ailleurs/outil-3d.exe", ["kernel32.dll"])
games.update_config(lambda c: (c["FOLDER"].append(str(extra)), c["HIDDEN"].append(str(hidden))))
assert games.add_game(manual) == manual.resolve()
try:
    games.add_game(work / "texte.exe")
    raise AssertionError("un fichier texte a été ajouté")
except ValueError:
    pass
config = games.load_config()
assert config["FOLDER"] == [str(extra)] and config["ADDED"] == [str(manual)], config

found = {Path(g["exe"]).name: g for g in games.scan()}
expected = {"superjeu.exe": ("Super jeu", "folder", True), "vieux.exe": ("Vieux jeu", "folder", False),
            "witcher.exe": ("The Witcher: Enhanced Edition", "gog", True), "TinyGame.exe": ("Tiny Game", "itch", True),
            "racer.exe": ("Racer", "installed", True), "plateformes.exe": ("Plateformes", "folder", True),
            "outil-3d.exe": ("Outil-3d", "manual", True)}
assert set(found) == set(expected), sorted(found)
for name, (title, kind, wide) in expected.items():
    game = found[name]
    assert (game["name"], game["source"], game["wide"]) == (title, kind, wide), game
    assert game["id"] == games.game_id(game["exe"]) and len(game["id"]) == 16
games.remove_game(str(manual))
games.remove_game(str(racer))
config = games.load_config()
assert str(manual) not in config["ADDED"] and str(racer) in config["HIDDEN"], config
assert {Path(g["exe"]).name for g in games.scan()} == set(expected) - {"outil-3d.exe", "racer.exe"}
games.update_config(lambda c: c["HIDDEN"].remove(str(racer)))
# Lignes inconnues ou dangereuses ignorées, chemins relatifs refusés.
games.config_path().write_text("PLAYTIME=2\nFOLDER=relatif\nFOLDER=/ok\nrm -rf /\nAUTOSCAN=0\n")
config = games.load_config()
assert config["PLAYTIME"] == "1" and config["FOLDER"] == ["/ok"] and config["AUTOSCAN"] == "0", config
games.update_config(lambda c: c.update(FOLDER=[str(extra)], HIDDEN=[str(hidden)], AUTOSCAN="1"))

# ── Temps de jeu : séances écrites par le runtime (runtime/src/playtime.rs) ──
now = int(time.time())
sessions = games.data_dir() / "sessions"
sessions.mkdir(parents=True)
(sessions / "10-1.session").write_text(f"exe={super_jeu}\nstart={now - 4000}\nlast={now - 400}\n")
(sessions / "11-2.session").write_text(f"exe={super_jeu}\nstart={now - 300}\nlast={now - 10}\n")
(sessions / "12-3.session").write_text(f"exe={witcher}\nstart={now - 1000}\nlast={now - 940}\n")
(sessions / "13-4.session").write_text("exe=relatif.exe\nstart=1\nlast=2\n")
(sessions / "14-5.session").write_text(f"exe={tiny}\nstart={now}\nlast={now - 50}\n")
times = games.playtime(now)
assert times[str(super_jeu)] == {"seconds": 3600 + 290, "last": now - 10, "running": True}, times
assert times[str(witcher)] == {"seconds": 60, "last": now - 940, "running": False}, times
assert str(tiny) not in times and "relatif.exe" not in times
assert sorted(p.name for p in sessions.iterdir()) == ["11-2.session"], "séances finies repliées dans le total"
assert games.playtime(now) == times, "relecture identique"
later = games.playtime(now + 500)
assert later[str(super_jeu)] == {"seconds": 3890, "last": now - 10, "running": False}, later
assert not any(sessions.iterdir())
total = (games.data_dir() / "totals.tsv").read_text()
assert f"3890\t{now - 10}\t{super_jeu}\n" in total, total
assert games.duration_text(30) == "moins d’une minute" and games.duration_text(125) == "2 min"
assert games.duration_text(3890) == "1 h 04 min"
summary = games.summary(found["superjeu.exe"], later)
assert summary.startswith("1 h 04 min · Joué le "), summary
assert games.summary(found["vieux.exe"], later).startswith("Programme 32 bits")
assert games.summary(found["TinyGame.exe"], later) == "Jamais lancé"
assert games.summary(found["superjeu.exe"], times).startswith("En cours · ")
games.forget_playtime()
assert games.playtime() == {}

# ── Raccourcis : menu Démarrer et bureau ──
game = found["superjeu.exe"]
name = games.pin(game)
assert name == f"winlux-game-{game['id']}.desktop", name
launcher = (home / ".local/share/applications" / name).read_text()
assert "Categories=Game;" in launcher and f"X-Winlux-Executable={super_jeu}" in launcher, launcher
assert 'Exec="/usr/bin/winlux-open" "' + str(super_jeu) + '"' in launcher, launcher
start = json.loads((home / ".config/winlux/start.json").read_text())
assert name in start["pins"] and "winlux-explorer.desktop" in start["pins"], start
assert games.pin(game) == name and json.loads((home / ".config/winlux/start.json").read_text())["pins"].count(name) == 1
# Un lanceur Winlux existant pour ce .exe est réutilisé plutôt que dupliqué.
existing = home / ".local/share/applications/winlux-pe-abc.desktop"
existing.write_text(f"[Desktop Entry]\nName=Witcher\nX-Winlux-Executable={witcher}\n")
assert games.pin(found["witcher.exe"]) == "winlux-pe-abc.desktop"
(home / ".config/user-dirs.dirs").write_text('XDG_DESKTOP_DIR="$HOME/Bureau"\n')
odd = dict(game, name='Jeu "spécial" $HOME / 100%')
shortcut = games.desktop_shortcut(odd)
assert shortcut.parent == home / "Bureau" and os.access(shortcut, os.X_OK), shortcut
text = shortcut.read_text()
assert 'Name=Jeu "spécial" $HOME / 100%' in text and "/" not in shortcut.name.replace(".desktop", ""), text

# ── Traductions : chaque texte affiché existe en anglais, espagnol et allemand ──
code = source.read_text()
shown = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', code)) | set(re.findall(r'_t\(\{[^}]*\}', code) and
                                                                     ["GOG", "itch.io", "Installé", "Dossier de jeux", "Ajouté à la main"])
missing = [text for text in shown if text not in games._TR or not all(games._TR[text])]
assert not missing, missing
pages = (project / "scripts/winlux-settings-pages.py").read_text()
body = pages[pages.index("def page_games"):pages.index("def sections_graphics")]
page_texts = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', body))
table = pages[:pages.index("def _t(")]
for text in page_texts:
    key = text.encode().decode("unicode_escape").encode("latin-1").decode("utf-8")
    assert repr(key)[1:-1] in table or key in table, f"traduction manquante : {key}"
desktop = (project / "integration/native/applications/winlux-games.desktop").read_text()
for lang in ("en", "es", "de"):
    assert f"Name[{lang}]=" in desktop and f"Comment[{lang}]=" in desktop, lang
for column in range(3):
    games._LANG = ("en", "es", "de")[column]
    assert games._t("Jouer") == games._TR["Jouer"][column]
games._LANG = "fr"

print(f"OK : {len(found)} jeux détectés (GOG, itch.io, dossiers, installés, ajout manuel), temps de jeu, raccourcis, 4 langues")

# ── Fenêtre (sous Xvfb) ──
if "--gtk" in sys.argv:
    games.save_cache(games.scan())
    sessions.mkdir(parents=True, exist_ok=True)
    (sessions / "20-1.session").write_text(f"exe={witcher}\nstart={now - 9000}\nlast={now - 5}\n")
    (sessions / "21-1.session").write_text(f"exe={super_jeu}\nstart={now - 90000}\nlast={now - 80000}\n")
    games.update_config(lambda c: c.update(AUTOSCAN="0"))
    window, Gtk = games.run_window()
    from gi.repository import GLib
    window.resize(1100, 700)
    result = {}

    def check():
        cards = window.flow.get_children()
        result["cards"] = len(cards)
        result["first"] = cards[0].game["name"] if cards else None
        window.search.set_text("tiny")
        while Gtk.events_pending(): Gtk.main_iteration()
        result["filtered"] = [c.game["name"] for c in cards if window.visible(c)]
        window.search.set_text("")
        while Gtk.events_pending(): Gtk.main_iteration()
        capture = next((a for a in sys.argv[1:] if a.endswith(".png")), None)
        if capture:
            GLib.timeout_add(400, lambda: shoot(capture))
        else:
            Gtk.main_quit()
        return False

    def shoot(path):
        gdk_window = window.get_window()
        from gi.repository import Gdk
        pixbuf = Gdk.pixbuf_get_from_window(gdk_window, 0, 0, gdk_window.get_width(), gdk_window.get_height())
        pixbuf.savev(path, "png", [], [])
        Gtk.main_quit()
        return False
    GLib.timeout_add(1200, check)
    Gtk.main()
    assert result["cards"] == len(expected) - 1, result
    assert result["first"] == "The Witcher: Enhanced Edition", result  # en cours : en tête des récents
    assert result["filtered"] == ["Tiny Game"], result
    print(f"OK : fenêtre Jeux, {result['cards']} cartes, tri par partie récente, recherche")
