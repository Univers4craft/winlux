#!/usr/bin/env python3
"""Validate real PE resources, registry/shortcuts, stale entries and Exec quoting."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import time

project = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("catalogue", project / "scripts/winlux-applications.py")
catalogue = importlib.util.module_from_spec(spec)
spec.loader.exec_module(catalogue)

def fixture():
    data = bytearray(0x600)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", data, 0x84, 0x8664, 1)
    struct.pack_into("<HH", data, 0x94, 0xf0, 0x22)
    struct.pack_into("<H", data, 0x98, 0x20b)
    struct.pack_into("<II", data, 0x98 + 112 + 16, 0x1000, 0x200)
    struct.pack_into("<IIII", data, 0x188 + 8, 0x400, 0x1000, 0x400, 0x200)
    def directory(offset, entries):
        struct.pack_into("<H", data, 0x200 + offset + 14, len(entries))
        for index, entry in enumerate(entries): struct.pack_into("<II", data, 0x200 + offset + 16 + index * 8, *entry)
    directory(0, [(3, 0x80000040), (14, 0x80000080)])
    directory(0x40, [(1, 0x80000060)])
    directory(0x60, [(1033, 0xa0)])
    directory(0x80, [(1, 0x800000c0)])
    directory(0xc0, [(1033, 0xe0)])
    image = struct.pack("<IIIHHIIIIII", 40, 1, 2, 1, 32, 0, 8, 0, 0, 0, 0) + bytes([220, 130, 190, 255]) + bytes(4)
    group = struct.pack("<HHHBBBBHHIH", 0, 1, 1, 1, 1, 0, 0, 1, 32, len(image), 1)
    struct.pack_into("<IIII", data, 0x2a0, 0x1100, len(image), 0, 0)
    struct.pack_into("<IIII", data, 0x2e0, 0x1140, len(group), 0, 0)
    data[0x300:0x300 + len(image)] = image
    data[0x340:0x340 + len(group)] = group
    return data

with tempfile.TemporaryDirectory() as temporary:
    home = Path(temporary)
    catalogue.HOME, catalogue.DATA = home, home / ".local/share"
    catalogue.STATE, catalogue.APPS = catalogue.DATA / "winlux", catalogue.DATA / "applications"
    exe = home / 'Bonjour é "test" $() ; &.exe'
    exe.write_bytes(fixture())
    assert catalogue.valid_exe(exe)
    icon = catalogue.pe_icon(exe)
    assert icon[:6] == struct.pack("<HHH", 0, 1, 1) and len(icon) == 70
    catalogue.register(exe)
    catalogue.register(exe)
    assert len(catalogue.records(home)) == 1
    launcher = catalogue.launcher(exe, "Bonjour Windows")
    assert (catalogue.APPS / launcher).is_file()
    assert str(home / ".cache/winlux/icons") in (catalogue.APPS / launcher).read_text()
    target = catalogue.APPS / launcher
    target.write_text(target.read_text().replace("Name=Bonjour Windows", "Name=Mon programme"))
    catalogue.launcher(exe, "Bonjour Windows")
    assert "Name=Mon programme" in target.read_text()
    assert catalogue.command_arguments('one "deux mots" "C:\\fin\\\\"') == ["one", "deux mots", "C:\\fin\\"]

    links = home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs"
    links.mkdir(parents=True)
    data = bytearray(76)
    struct.pack_into("<I", data, 0, 76)
    struct.pack_into("<I", data, 20, 8 | 32 | 128)
    for value in [str(exe), '--message "deux mots"']:
        encoded = value.encode("utf-16-le")
        data.extend(struct.pack("<H", len(encoded) // 2) + encoded)
    (links / "Programme installé.lnk").write_bytes(data)
    assert list(catalogue.shortcuts(home)) == [(exe, "Programme installé", ["--message", "deux mots"])]
    # Raccourci d'éditeur vers C:\Users\<compte>\AppData\… (Claude pour Windows, #1188/#1189) :
    # le dossier personnel lui-même, pas ~/Users/<compte>/…
    claude = home / "AppData/Local/AnthropicClaude/app-0.14.10/claude.exe"
    claude.parent.mkdir(parents=True)
    claude.write_bytes(fixture())
    (links / "Anthropic PBC").mkdir()
    data = bytearray(76)
    struct.pack_into("<I", data, 0, 76)
    struct.pack_into("<I", data, 20, 8 | 128)
    encoded = f"C:\\Users\\{home.name.upper()}\\AppData\\Local\\AnthropicClaude\\app-0.14.10\\claude.exe".encode("utf-16-le")
    data.extend(struct.pack("<H", len(encoded) // 2) + encoded)
    (links / "Anthropic PBC/Claude.lnk").write_bytes(data)
    assert (claude, "Claude", []) in list(catalogue.shortcuts(home)), list(catalogue.shortcuts(home))
    assert catalogue.windows_path(f"C:\\Users\\{home.name}\\Desktop\\a.txt", home) == catalogue.desktop_dir(home) / "a.txt"
    assert catalogue.windows_path("C:\\Users\\autre\\a.exe", home) == home / "Users/autre/a.exe"
    assert catalogue.windows_path("C:\\Program Files\\X\\a.exe", home) == home / "Program Files/X/a.exe"
    (links / "Anthropic PBC/Claude.lnk").unlink()
    (links / "Anthropic PBC").rmdir()
    claude.unlink()
    key = "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Test"
    lines = ["WINLUX-REGISTRY-1"]
    for name, value in [("DisplayName", "Programme installé"), ("DisplayIcon", f'"{exe}",0')]:
        lines.append("\t".join(["V", key.encode().hex(), name.encode().hex(), "1", (value + "\0").encode("utf-16-le").hex()]))
    (catalogue.STATE / "registry.tsv").write_text("\n".join(lines))
    assert catalogue.installed_registry(home) == [(exe, "Programme installé")]

    # File associations declared in the registry (HKLM/HKCU\Software\Classes, legacy hkcr).
    catalogue.MIMEAPPS = home / ".config/mimeapps.list"
    def value(key, name, text, kind="1"):
        return "\t".join(["V", key.encode().hex(), name.encode().hex(), kind, (text + "\0").encode("utf-16-le").hex()])
    editor = home / "Program Files/Éditeur Winlux/editeur.exe"
    editor.parent.mkdir(parents=True)
    editor.write_bytes(fixture())
    command = '"C:\\Program Files\\Éditeur Winlux\\editeur.exe" /open "%1"'
    lines += [value("HKLM\\Software\\Classes\\.wxdoc", "", "Winlux.Document"),
              value("HKLM\\Software\\Classes\\Winlux.Document", "", "Document Winlux"),
              value("HKLM\\Software\\Classes\\Winlux.Document\\shell\\open\\command", "", command),
              value("hkcr\\.txt\\OpenWithProgids", "Winlux.Document", ""),
              value("HKCU\\Software\\Classes\\.rien", "", "Absent.Program")]
    (catalogue.STATE / "registry.tsv").write_text("WINLUX-REGISTRY-2\n" + "\n".join(lines[1:]))
    found = catalogue.associations(home)
    assert sorted(found) == [".txt", ".wxdoc"], found
    assert found[".wxdoc"]["handlers"] == [(editor, ("/open", None), "winlux.document")] and found[".wxdoc"]["default"] == 0
    assert found[".txt"]["default"] is None and found[".wxdoc"]["label"] == "Document Winlux"
    assert catalogue.handler('%ProgramFiles%\\Absent\\app.exe "%1"', home) is None
    catalogue.refresh()
    assoc = list(catalogue.APPS.glob("winlux-assoc-*.desktop"))
    assert len(assoc) == 1, assoc
    text = assoc[0].read_text()
    assert "NoDisplay=true" in text and "text/plain;" in text and "application/x-winlux-wxdoc;" in text, text
    assert '"desktop-open"' in text and '"/open" %f' in text, text
    assert "Name=editeur" in text
    package = (catalogue.DATA / "mime/packages/winlux-associations.xml").read_text()
    assert 'glob pattern="*.wxdoc"' in package and "Document Winlux" in package
    defaults = catalogue.read_mimeapps()
    assert catalogue.default_of(defaults, "application/x-winlux-wxdoc") == assoc[0].name
    assert catalogue.default_of(defaults, "text/plain") is None  # Only listed, never imposed.
    # The user's later choice is kept on the next refresh.
    catalogue.promote(defaults, "application/x-winlux-wxdoc", "org.xfce.mousepad.desktop")
    catalogue.write_mimeapps(defaults)
    catalogue.refresh()
    assert catalogue.default_of(catalogue.read_mimeapps(), "application/x-winlux-wxdoc") == "org.xfce.mousepad.desktop"
    catalogue.promote(defaults, "application/x-winlux-wxdoc", assoc[0].name)
    catalogue.write_mimeapps(defaults)

    # Desktop .lnk: target, arguments, working folder and icon for winlux-open.
    shortcut = bytearray(76)
    struct.pack_into("<I", shortcut, 0, 76)
    struct.pack_into("<I", shortcut, 20, 16 | 32 | 64 | 128)
    struct.pack_into("<i", shortcut, 56, 0)
    struct.pack_into("<I", shortcut, 20, 8 | 16 | 32 | 64 | 128)
    for item in [str(exe), "C:\\Documents", '-x "a b"', str(exe)]:
        encoded = item.encode("utf-16-le")
        shortcut.extend(struct.pack("<H", len(encoded) // 2) + encoded)
    desktop_link = home / "Bureau/Programme.lnk"
    desktop_link.parent.mkdir()
    desktop_link.write_bytes(shortcut)
    target, arguments, workdir, icon, index = catalogue.lnk_resolve(desktop_link, home)
    assert (target, arguments, workdir, icon, index) == (exe, ["-x", "a b"], home / "Documents", exe, 0)
    # On the desktop the shortcut is labelled « Programme », not « Programme.lnk ».
    (home / "Bureau/.hidden").write_text("note.txt\n")
    catalogue.desktop_shortcuts(home)
    entries = list((home / "Bureau").glob("winlux-lnk-*.desktop"))
    assert len(entries) == 1 and "Name=Programme\n" in entries[0].read_text(), entries
    assert "winlux-open" in entries[0].read_text() and os.access(entries[0], os.X_OK)
    assert (home / "Bureau/.hidden").read_text().splitlines() == ["note.txt", "Programme.lnk"]
    catalogue.desktop_shortcuts(home)
    assert (home / "Bureau/.hidden").read_text().splitlines() == ["note.txt", "Programme.lnk"]
    moved = desktop_link.with_name("Ailleurs.bin")
    desktop_link.rename(moved)
    catalogue.desktop_shortcuts(home)
    assert not list((home / "Bureau").glob("winlux-lnk-*.desktop")), "launcher of a removed shortcut kept"
    assert (home / "Bureau/.hidden").read_text().splitlines() == ["note.txt"]
    moved.rename(desktop_link)
    try:
        import gi  # noqa: F401
        gi.require_version("GdkPixbuf", "2.0")
        from gi.repository import GdkPixbuf
    except (ImportError, ValueError): GdkPixbuf = None
    if GdkPixbuf is not None:
        png = home / "thumbnail.png"
        catalogue.HOME = home
        catalogue.thumbnail(desktop_link, png, 128)
        assert png.read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"

    # Resident catalogue (#214): refresh() only when a watched input changes.
    catalogue.HOME = home
    before = catalogue.watched_inputs()
    assert catalogue.watched_inputs() == before
    sub = home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Nouveau dossier"
    sub.mkdir(parents=True, exist_ok=True)
    after = catalogue.watched_inputs()
    assert after != before, "un dossier ajouté au menu Démarrer doit relancer le catalogue"
    (sub / "Programme.lnk").write_bytes(b"x")
    assert catalogue.watched_inputs() != after, "un raccourci ajouté dans un sous-dossier doit relancer le catalogue"
    calls = {"refresh": 0, "icons": 0, "ticks": 0}
    saved = catalogue.refresh, catalogue.window_icons, catalogue.time.sleep
    class Stop(Exception): pass
    def tick(_seconds):
        calls["ticks"] += 1
        if calls["ticks"] == 2: (home / ".local/share/winlux").mkdir(parents=True, exist_ok=True); (home / ".local/share/winlux/launchers.json").write_text("[]")
        if calls["ticks"] >= 4: raise Stop
    catalogue.refresh = lambda: calls.__setitem__("refresh", calls["refresh"] + 1)
    catalogue.window_icons = lambda: calls.__setitem__("icons", calls["icons"] + 1)
    catalogue.time.sleep = tick
    try: catalogue.watch(full_pass=3600)
    except Stop: pass
    finally: catalogue.refresh, catalogue.window_icons, catalogue.time.sleep = saved
    assert calls["icons"] == 4, calls
    assert calls["refresh"] == 2, f"refresh au démarrage puis une fois après le changement, pas à chaque tour : {calls}"
    (sub / "Programme.lnk").unlink(); sub.rmdir()

    # GLib itself parses the Desktop Entry Exec: no shell expansion is involved.
    if shutil.which("gio"):
        output = home / "arguments.json"
        helper = home / "launcher with spaces.py"
        helper.write_text('#!/usr/bin/python3\nimport json,os,sys\nopen(os.environ["WINLUX_ARGUMENTS_TEST"],"w").write(json.dumps(sys.argv[1:]))\n')
        helper.chmod(0o755)
        catalogue.EXE_LAUNCHER = str(helper)
        launcher = catalogue.launcher(exe, "Argument test", extra=["deux mots", "$()`; &"])
        launched = subprocess.run(["gio", "launch", str(catalogue.APPS / launcher)], env={**os.environ, "WINLUX_ARGUMENTS_TEST": str(output)}, capture_output=True, text=True)
        assert launched.returncode == 0, launched.stderr + (catalogue.APPS / launcher).read_text()
        for attempt in range(30):
            if output.exists(): break
            time.sleep(0.1)
        assert json.loads(output.read_text()) == ["desktop", str(exe), "deux mots", "$()`; &"]
    # Aide et site web d'un programme (#992) : entrées du menu, pas « Documents ».
    folder = links / "PuTTY (64-bit)"
    folder.mkdir()
    (home / "putty").mkdir(exist_ok=True)
    (home / "putty/putty.chm").write_bytes(b"ITSF")
    (home / "putty/website.url").write_text("[InternetShortcut]\r\nURL=https://example.org/putty/\r\n")
    for name, target in [("PuTTY Manual", home / "putty/putty.chm"), ("PuTTY Web Site", home / "putty/website.url")]:
        data = bytearray(76)
        struct.pack_into("<I", data, 0, 76)
        struct.pack_into("<I", data, 20, 8 | 128)
        encoded = str(target).encode("utf-16-le")
        data.extend(struct.pack("<H", len(encoded) // 2) + encoded)
        (folder / f"{name}.lnk").write_bytes(data)
    (folder / "Forum.url").write_text("[InternetShortcut]\nURL=javascript:alert(1)\n")
    (folder / "Aide en ligne.url").write_text("[InternetShortcut]\nURL=https://example.org/aide\n")
    found_links = sorted(catalogue.link_shortcuts(home))
    assert found_links == [("Aide en ligne", "https://example.org/aide", "web-browser"),
                           ("PuTTY Manual", str(home / "putty/putty.chm"), "help-browser"),
                           ("PuTTY Web Site", "https://example.org/putty/", "web-browser")], found_links
    assert all(name != "Programme installé" for name, _, _ in found_links), "un .exe compté deux fois"
    catalogue.refresh()
    link_entries = {path.read_text().split("Name=", 1)[1].split("\n", 1)[0]: path.read_text()
                    for path in catalogue.APPS.glob("winlux-link-*.desktop")}
    assert sorted(link_entries) == ["Aide en ligne", "PuTTY Manual", "PuTTY Web Site"], sorted(link_entries)
    assert 'Exec=/usr/bin/xdg-open "https://example.org/putty/"' in link_entries["PuTTY Web Site"]
    assert "Categories=Utility;WinluxWindows;" in link_entries["PuTTY Manual"]
    (folder / "Aide en ligne.url").unlink()
    catalogue.refresh()
    assert len(list(catalogue.APPS.glob("winlux-link-*.desktop"))) == 2, "lanceur d'un raccourci supprimé gardé"
    menu = (project / "scripts/winlux-start-menu.py").read_text()
    assert '"/Start Menu/Programs" in directory' in menu and '("winlux-pe-", "winlux-link-")' in menu
    exe.unlink()
    editor.unlink()
    catalogue.refresh()
    assert not list(catalogue.APPS.glob("winlux-pe-*.desktop"))
    assert not list(catalogue.APPS.glob("winlux-assoc-*.desktop"))
    # Uninstalled: the previous choice (here the editor picked by the user) applies again.
    assert catalogue.default_of(catalogue.read_mimeapps(), "application/x-winlux-wxdoc") == "org.xfce.mousepad.desktop"
print("OK: PE icon, registered programs, Windows shortcuts, file associations, defaults, quoting, stale entries and resident catalogue")
