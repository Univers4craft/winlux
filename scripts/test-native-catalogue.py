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
    key = "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Test"
    lines = ["WINLUX-REGISTRY-1"]
    for name, value in [("DisplayName", "Programme installé"), ("DisplayIcon", f'"{exe}",0')]:
        lines.append("\t".join(["V", key.encode().hex(), name.encode().hex(), "1", (value + "\0").encode("utf-16-le").hex()]))
    (catalogue.STATE / "registry.tsv").write_text("\n".join(lines))
    assert catalogue.installed_registry(home) == [(exe, "Programme installé")]

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
    exe.unlink()
    catalogue.refresh()
    assert not list(catalogue.APPS.glob("winlux-pe-*.desktop"))
print("OK: PE icon, registered programs, Windows shortcuts, quoting and stale entries")
