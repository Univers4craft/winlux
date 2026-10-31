#!/usr/bin/python3
"""Offline checks of the in-place update guard: restore point, file verification,
trial boots and automatic return to the previous version. No real package is
installed: dpkg and reboot are simulated inside a temporary directory."""
import importlib.util
import io
import os
from contextlib import redirect_stdout
from pathlib import Path
import subprocess
import sys
import tempfile

directory = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("winlux_update_apt", directory / "winlux-update-apt.py")
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)

with tempfile.TemporaryDirectory(prefix="update-guard.") as temporary:
    root = Path(temporary)
    state, run, cache = root / "state", root / "run", root / "cache"
    for folder in (state, run, cache): folder.mkdir()
    version_file = root / "version"; cmdline = root / "cmdline"
    updater.STATE, updater.RUN, updater.CACHE, updater.CMDLINE = state, run, cache, cmdline
    updater.TRIAL = state / "update-trial"; updater.BLOCKED = state / "update-blocked"
    updater.ROLLED_BACK = state / "update-rolled-back"; updater.RESTORE_POINT = state / "update-restore-point"
    updater.installed_version = lambda: version_file.read_text().strip()
    # A package here is a text file holding its version; dpkg --install "installs" it.
    updater.package_version = lambda package: Path(package).read_text().strip().removeprefix("v") if Path(package).is_file() else ""
    reboots = []
    updater.restart = lambda: reboots.append(True)
    real_run = subprocess.run
    def fake_run(command, *args, **kwargs):
        if command[:1] == ["dpkg"] and "--install" in command:
            version_file.write_text(Path(command[-1]).read_text())
            return subprocess.CompletedProcess(command, 0, "", "")
        return real_run(command, *args, **kwargs)
    updater.subprocess.run = fake_run

    def install_new(new, old):
        """State left by --apply: old package kept as previous, new one on trial."""
        version_file.write_text(new + "\n")
        (cache / "previous.deb").write_text(old + "\n"); (cache / "installed.deb").write_text(new + "\n")
        updater.write_fields(updater.TRIAL, {"version": new, "previous": old, "boots": "0"})

    # 1. A diagnostic (non-interactive) boot never counts as a desktop attempt.
    install_new("v0.3.0", "v0.2.9")
    cmdline.write_text("root=/dev/sda2 init=/init\n")
    updater.boot_guard()
    assert updater.fields(updater.read(updater.TRIAL))["boots"] == "0"

    # 2. Two interactive boots without a desktop are allowed, the third goes back.
    cmdline.write_text("root=/dev/sda2 init=/init winlux.interactive=1\n")
    updater.boot_guard(); updater.boot_guard()
    assert updater.fields(updater.read(updater.TRIAL))["boots"] == "2" and not reboots
    with redirect_stdout(io.StringIO()): updater.boot_guard()
    assert reboots and updater.installed_version() == "v0.2.9", "no automatic return after two failed boots"
    assert not updater.TRIAL.exists()
    assert (cache / "installed.deb").read_text().strip() == "v0.2.9"
    assert (cache / "previous.deb").read_text().strip() == "v0.3.0", "rejected version must stay restorable"
    assert updater.skipped_automatically("v0.3.0") and not updater.skipped_automatically("v0.3.1")
    report = io.StringIO()
    with redirect_stdout(report): updater.listing()
    assert "Version écartée (le bureau n’a pas démarré) : v0.3.0" in report.getvalue()
    assert "Version restaurée automatiquement : v0.2.9" in report.getvalue()

    # 3. A desktop that opens confirms the version: nothing is rolled back later.
    reboots.clear(); install_new("v0.3.1", "v0.2.9")
    with redirect_stdout(io.StringIO()): updater.confirm_boot()
    assert not updater.TRIAL.exists()
    for _ in range(4): updater.boot_guard()
    assert not reboots and updater.installed_version() == "v0.3.1"
    assert not updater.session_failed(), "a confirmed version must never be replaced"

    # 4. A desktop session that fails to start goes back immediately.
    install_new("v0.3.2", "v0.3.1")
    with redirect_stdout(io.StringIO()): assert updater.session_failed()
    assert reboots and updater.installed_version() == "v0.3.1"

    # 5. Without a kept previous version the boot continues on the new one.
    reboots.clear(); install_new("v0.3.3", "v0.3.2"); (cache / "previous.deb").unlink()
    updater.write_fields(updater.TRIAL, {"version": "v0.3.3", "boots": "2"})
    with redirect_stdout(io.StringIO()), open(os.devnull, "w") as quiet:
        sys.stderr, saved = quiet, sys.stderr
        try: updater.boot_guard()
        finally: sys.stderr = saved
    assert not reboots and updater.installed_version() == "v0.3.3" and not updater.TRIAL.exists()

    # 6. File verification ignores edited configuration files only.
    def verify(output):
        updater.subprocess.run = lambda *a, **k: subprocess.CompletedProcess(a[0], 0, output, "")
        try: return updater.verify_installed()
        finally: updater.subprocess.run = fake_run
    assert verify("??5?????? c /etc/apt/apt.conf.d/90winlux-desktop\n") == []
    assert verify("??5??????   /usr/bin/winlux\nmissing     /usr/bin/winlux-settings\n") == [
        "??5??????   /usr/bin/winlux", "missing     /usr/bin/winlux-settings"]

    # 7. Restore points: one per update, only the three newest update points kept,
    #    points made by the user are never deleted.
    tools = root / "bin"; tools.mkdir(); points = root / "points"; points.mkdir()
    (points / "rp-20260101-000000").write_text("Mon point")
    tool = tools / "winlux-restore"
    tool.write_text('#!/bin/sh\nset -eu\nd="$WINLUX_TEST_POINTS"\ncase "$1" in\n'
                    '  create) n=$(ls "$d" | wc -l); id=rp-2026110$n-000000; printf "%s" "$2" > "$d/$id"; printf "RESTORE_CREATED\\nid=%s\\n" "$id" ;;\n'
                    '  list) for p in "$d"/*; do printf "%s|%s|date|4K\\n" "${p##*/}" "$(cat "$p")"; done ;;\n'
                    '  delete) rm "$d/$2" ;;\nesac\n')
    tool.chmod(0o755)
    os.environ["PATH"] = f"{tools}:{os.environ['PATH']}"; os.environ["WINLUX_TEST_POINTS"] = str(points)
    for version in ("v0.4.0", "v0.4.1", "v0.4.2", "v0.4.3"): updater.restore_point(version)
    names = sorted(path.read_text() for path in points.iterdir())
    assert names == ["Avant Winlux v0.4.1", "Avant Winlux v0.4.2", "Avant Winlux v0.4.3", "Mon point"], names
    assert updater.fields(updater.read(updater.RESTORE_POINT))["id"].startswith("rp-")
    assert updater.fields(updater.read(run / "update-progress"))["phase"] == "restore-point"

print("OK: restore point, file verification, trial boots and automatic return to the previous version")
