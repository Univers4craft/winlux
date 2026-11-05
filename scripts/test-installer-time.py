#!/usr/bin/env python3
"""Date, heure et fuseau : justes dès l'installation.

Sur le portable de damien (v0.2.18) : horloge matérielle en 2015, système en
UTC (« timezone » vide à l'installation) → navigateurs refusés (« certificat
pas encore valide ») et une heure de retard. Le test vérifie, sans GTK :
- fuseau proposé d'après la langue, liste des fuseaux, conversion de la
  date saisie dans le fuseau choisi, horloge et RTC réglées ;
- la page Réseau : heure synchronisée dès la connexion, saisie appliquée en
  quittant la page, synchro avant les pilotes ; textes en 6 langues ;
- l'init : fuseau de la langue pour les systèmes installés sans fuseau ;
- winlux-control status : le fuseau réel, plus « Europe/Paris » inventé.
"""
import ast
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
import types

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/winlux-installer.py").read_text(encoding="utf-8")
tree = ast.parse(source)
names = {"TIMEZONE_BY_LOCALE", "TIMEZONE_BY_LANGUAGE", "ZONEINFO", "default_timezone", "list_timezones",
         "local_to_epoch", "zone_now", "set_system_clock", "sync_clock_now"}
body = [node for node in tree.body
        if (isinstance(node, ast.FunctionDef) and node.name in names)
        or (isinstance(node, ast.Assign) and any(getattr(t, "id", "") in names for t in node.targets))]
assert len(body) == len(names), "aides de l'heure introuvables dans l'installateur"
ns = {"os": os, "time": time, "shutil": shutil, "subprocess": subprocess}
exec(compile(ast.Module(body=body, type_ignores=[]), "installer", "exec"), ns)

assert ns["default_timezone"]("fr_FR") == "Europe/Paris"
assert ns["default_timezone"]("fr_CA") == "America/Toronto"
assert ns["default_timezone"]("de_DE") == "Europe/Berlin"
assert ns["default_timezone"]("es_ES") == "Europe/Madrid"
assert ns["default_timezone"]("en_US") == "America/New_York"
assert ns["default_timezone"]("xx_YY") == "Europe/Paris"
zones = ns["list_timezones"]()
assert "Europe/Paris" in zones and "UTC" in zones and zones == sorted(zones)
print("fuseau d'après la langue (fr_FR → Europe/Paris), liste des fuseaux")

# 3 novembre 2026, 20 h 19 à Paris (UTC+1) = 19 h 19 UTC : l'heure de damien.
epoch = ns["local_to_epoch"](2026, 11, 3, 20, 19, "Europe/Paris")
assert time.gmtime(epoch)[:5] == (2026, 11, 3, 19, 19), time.gmtime(epoch)
assert time.gmtime(ns["local_to_epoch"](2026, 7, 1, 12, 0, "Europe/Paris"))[3] == 10   # heure d'été
assert ns["zone_now"]("Europe/Paris").tm_year >= 2024
print("date saisie à Paris convertie en UTC (hiver +1, été +2)")

settings, ioctls = [], []
import fcntl
real_ioctl, real_settime = fcntl.ioctl, time.clock_settime
fcntl.ioctl = lambda fd, request, data: ioctls.append(request)
time.clock_settime = lambda clock, value: settings.append(value)
try:
    assert ns["set_system_clock"](epoch, rtc=os.devnull) is True
    assert settings == [epoch] and ioctls == [0x4024700a]
    assert ns["set_system_clock"](epoch, rtc="/nonexistent/rtc") is False
finally:
    fcntl.ioctl, time.clock_settime = real_ioctl, real_settime
print("heure saisie : horloge du live et horloge matérielle réglées")

# ── Page Réseau ─────────────────────────────────────────────────────────
klass = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "InstallerApp")
methods = {node.name: ast.get_source_segment(source, node) for node in klass.body if isinstance(node, ast.FunctionDef)}
assert "self.build_time_section(box)" in methods["build_network_page"]
assert "self.start_clock_sync()" in methods["refresh_wifi_list"], "connexion établie sans synchro de l'heure"
assert "sync_clock_now()" in methods["start_clock_sync"]
assert "self.apply_time_choice()" in methods["on_next"]
assert "set_system_clock(" in methods["apply_time_choice"] and "local_to_epoch(" in methods["apply_time_choice"]
assert "self.follow_language_timezone()" in methods["update_ui_language"]
hw = methods["install_hardware_support"]
assert hw.index("sync_clock_now()") < hw.index('"/usr/bin/winlux-drivers", "auto"')
first = methods["configure_first_boot"]
assert "/etc/timezone" in first and "localtime" in first
print("page Réseau : heure par Internet dès la connexion, sinon saisie appliquée ; synchro avant les pilotes")
keys = ["time_title", "time_tz", "time_auto", "time_manual", "time_date", "time_hour", "time_auto_ok",
        "time_auto_wait", "time_auto_failed", "time_manual_hint"]
for key in keys:
    count = len(re.findall(rf'^\s+"{key}":', source, re.M))
    assert count == 6, f"{key} : {count} langue(s) sur 6"
print("textes de la date et de l'heure en 6 langues")

# ── Init : fuseau des systèmes installés sans fuseau ────────────────────
build = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = build[build.index('cat >"$stage/init" <<'):]
block = re.search(r"^if test -e /etc/winlux/install-config.json && ! test -s /etc/timezone; then\n.*?^fi\n", init, re.S | re.M)
assert block, "l'init ne règle pas le fuseau des systèmes installés sans fuseau"
with tempfile.TemporaryDirectory() as tmp:
    tmp = pathlib.Path(tmp)
    for lang, expected in (("fr_FR", "Europe/Paris"), ("de_CH", "Europe/Zurich"), ("en_US", "America/New_York")):
        for sub in ("etc/winlux", "etc/default", "usr/share/zoneinfo/Europe", "usr/share/zoneinfo/America"):
            (tmp / sub).mkdir(parents=True, exist_ok=True)
        for zone in ("Europe/Paris", "Europe/Zurich", "America/New_York"):
            (tmp / "usr/share/zoneinfo" / zone).write_text("TZif")
        (tmp / "etc/winlux/install-config.json").write_text('{"timezone": null}')
        (tmp / "etc/default/locale").write_text(f"LANG={lang}.UTF-8\n")
        (tmp / "etc/timezone").unlink(missing_ok=True)
        localtime = tmp / "etc/localtime"
        if localtime.is_symlink() or localtime.exists():
            localtime.unlink()
        localtime.symlink_to("/usr/share/zoneinfo/Etc/UTC")
        code = block.group(0).replace("/etc/", f"{tmp}/etc/").replace("/usr/share/zoneinfo/", f"{tmp}/usr/share/zoneinfo/")
        code = code.replace("/bin/busybox ", "")
        subprocess.run(["sh", "-c", code], check=True)
        assert (tmp / "etc/timezone").read_text().strip() == expected, lang
        assert os.readlink(localtime).endswith(expected), os.readlink(localtime)
    # install-config.json vide (#433) : le fuseau vient quand même de la langue.
    (tmp / "etc/winlux/install-config.json").write_text("")
    (tmp / "etc/default/locale").write_text("LANG=fr_FR.UTF-8\n")
    (tmp / "etc/timezone").unlink(missing_ok=True)
    subprocess.run(["sh", "-c", code], check=True)
    assert (tmp / "etc/timezone").read_text().strip() == "Europe/Paris", "install-config.json vide : fuseau sauté"
    # Live (pas d'install-config.json) : rien n'est touché.
    (tmp / "etc/winlux/install-config.json").unlink()
    (tmp / "etc/timezone").unlink()
    subprocess.run(["sh", "-c", code], check=True)
    assert not (tmp / "etc/timezone").exists(), "le live ne doit pas recevoir de fuseau"
    (tmp / "etc/winlux/install-config.json").write_text("{}")
    # Un fuseau déjà choisi n'est jamais remplacé.
    (tmp / "etc/timezone").write_text("Asia/Tokyo\n")
    subprocess.run(["sh", "-c", code], check=True)
    assert (tmp / "etc/timezone").read_text().strip() == "Asia/Tokyo"
print("init : système installé en UTC sans fuseau → fuseau de sa langue, même avec install-config.json vide ; un choix existant est gardé")

control = (ROOT / "scripts/winlux-control.sh").read_text(encoding="utf-8")
line = next(line for line in control.splitlines() if "printf 'timezone=%s" in line)
assert "Europe/Paris" not in line and "readlink /etc/localtime" in line and "UTC" in line, line
print("winlux-control status : le fuseau réel (/etc/timezone, puis /etc/localtime, sinon UTC)")

# /etc/timezone lisible par le compte (testeur VM : écrit en 0600 sous umask 077).
assert 'os.chmod(f"{root_path}/etc/timezone", 0o644)' in source
assert "chmod 644 /etc/timezone" in (ROOT / "scripts/winlux-first-boot.sh").read_text(encoding="utf-8")
assert "chmod 644 /etc/timezone" in control
assert "/bin/busybox chmod 644 /etc/timezone" in block.group(0)
assert '"$root/etc/timezone"' in (ROOT / "scripts/winlux-network-perms.sh").read_text(encoding="utf-8")
print("/etc/timezone toujours en 0644 (installateur, premier démarrage, Paramètres, init, réparation)")
