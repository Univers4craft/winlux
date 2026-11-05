#!/usr/bin/env python3
"""Premier démarrage après une mise à jour : le bureau n'attend plus les caches.

Testeur VM (v0.2.18 → v0.2.19) : 34 s de logo seul dans `winlux-update
--boot`, update-mime-database refait en premier plan. Le test vérifie :
- les caches MIME et applications partent en arrière-plan (nice, session à
  part) et le repère de version n'est écrit qu'à leur fin ;
- chaque étape de --boot est chronométrée dans boot-update.log ;
- les paquets conservés (previous.deb, installed.deb) sont lisibles par le
  compte : `winlux-update-apt --list` affiche à nouveau la version précédente.
"""
import os
from pathlib import Path
import runpy
import stat
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
source = (HERE / "winlux-update-apt.py").read_text(encoding="utf-8")
updater = runpy.run_path(str(HERE / "winlux-update-apt.py"), run_name="update_first_boot_test")

boot = source[source.index('        if args == ["--boot"]:'):source.index('        if busy(): raise RuntimeError')]
assert 'run("update-mime-database"' not in boot and 'run("update-desktop-database"' not in boot, \
    "caches du bureau refaits en premier plan au démarrage"
assert "refresh_desktop_caches_later(version" in boot
for label in ("Version à l'essai", "Droits de la base et su", "Vérification dpkg", "Caches du bureau"):
    assert f'timed("{label}' in boot, label
assert "readable_cache()" in boot
print("--boot : caches du bureau en arrière-plan, étapes chronométrées dans boot-update.log")

with tempfile.TemporaryDirectory(prefix="update-first-boot.") as temporary:
    root = Path(temporary)
    bin_dir = root / "bin"; bin_dir.mkdir()
    for name, body in (("update-mime-database", "sleep 1; echo mime \"$@\" >>\"$WITNESS\""),
                       ("update-desktop-database", "echo desktop \"$@\" >>\"$WITNESS\"")):
        tool = bin_dir / name
        tool.write_text("#!/bin/sh\n" + body + "\n"); tool.chmod(0o755)
    marker = root / "apt-desktop-cache-version"
    log = root / "desktop-caches.log"
    witness = root / "witness"
    saved = dict(os.environ)
    os.environ["PATH"] = f"{bin_dir}:{os.environ['PATH']}"
    os.environ["WITNESS"] = str(witness)
    try:
        started = time.monotonic()
        assert updater["refresh_desktop_caches_later"]("v0.2.19", marker, log=log) is True
        assert time.monotonic() - started < 0.8, "le démarrage attend encore les caches"
        assert not marker.exists(), "repère écrit avant la fin des caches"
        deadline = time.monotonic() + 15
        while not marker.exists() and time.monotonic() < deadline: time.sleep(0.1)
    finally:
        os.environ.clear(); os.environ.update(saved)
    assert marker.read_text() == "v0.2.19\n", marker.read_text()
    assert witness.read_text().splitlines() == ["desktop /usr/share/applications", "mime /usr/share/mime"]
    print("caches MIME et applications refaits en arrière-plan, repère écrit à la fin")

    cache = root / "cache"; cache.mkdir(mode=0o700)
    for name in ("previous.deb", "installed.deb"):
        (cache / name).write_bytes(b"deb"); (cache / name).chmod(0o600)
    updater["readable_cache"](cache)
    assert stat.S_IMODE(cache.stat().st_mode) == 0o755
    for name in ("previous.deb", "installed.deb"):
        assert stat.S_IMODE((cache / name).stat().st_mode) == 0o644, name
    copied = cache / "copy.deb"
    updater["cache_copy"](cache / "previous.deb", copied)
    assert stat.S_IMODE(copied.stat().st_mode) == 0o644, oct(copied.stat().st_mode)
    print("previous.deb et installed.deb lisibles par le compte (anciens 0600 réparés, nouvelles copies en 0644)")
