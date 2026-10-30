#!/usr/bin/python3
"""Only inside the disposable native Winlux guest, never on the host."""
import os
from pathlib import Path
import runpy
import stat
import subprocess
import tempfile

def run(*args):
    try: return subprocess.check_output(args, stderr=subprocess.STDOUT, text=True, timeout=180)
    except subprocess.CalledProcessError as error:
        print(error.output, flush=True)
        raise

assert "winlux.native-smoke=1" in Path("/proc/cmdline").read_text().split(), "Disposable native diagnostic guest required"
assert Path("/usr/share/winlux/update-backend").read_text().strip() == "apt"
assert stat.S_IMODE(Path("/usr/bin/sudo").stat().st_mode) & stat.S_ISUID, "Native package modes were not restored"
assert run("dpkg", "--audit").strip() == ""
run("apt-get", "check")
release = run("uname", "-r").strip()
assert run("dpkg-query", "-W", "-f=${Version}", "winlux-kernel").strip() == release
assert not any(path.startswith(("/usr/lib/modules/", "/lib/modules/", "/boot/"))
               for path in run("dpkg-query", "-L", "winlux-desktop").splitlines())
with tempfile.TemporaryDirectory(prefix="apt-probe-", dir="/var/lib/winlux") as temporary:
    work = Path(temporary); package = work / "probe"
    (package / "DEBIAN").mkdir(parents=True)
    (package / "DEBIAN/control").write_text(
        "Package: winlux-apt-probe\nVersion: 1.0\nArchitecture: all\n"
        "Maintainer: Winlux <winlux@localhost>\nDescription: Native integration diagnostic\n")
    binary = package / "usr/bin/winlux-apt-probe"; binary.parent.mkdir(parents=True)
    binary.write_text("#!/bin/sh\nprintf 'native-apt-ok\\n'\n"); binary.chmod(0o755)
    entry = package / "usr/share/applications/winlux-apt-probe.desktop"; entry.parent.mkdir(parents=True)
    entry.write_text("[Desktop Entry]\nType=Application\nName=Native APT Probe\nExec=winlux-apt-probe\nTerminal=true\n")
    # PID 1 intentionally uses umask 077; Debian archives require public
    # directory metadata, independently of the private temporary work folder.
    for path in [package] + list(package.rglob("*")):
        path.chmod(0o755 if path.is_dir() or path == binary else 0o644)
    archive = work / "probe.deb"
    run("dpkg-deb", "--root-owner-group", "--build", str(package), str(archive))
    run("apt-get", "--no-remove", "-y", "install", str(archive))
    assert run("winlux-apt-probe").strip() == "native-apt-ok"
    from gi.repository import Gio
    assert any(app.get_id() == "winlux-apt-probe.desktop" for app in Gio.AppInfo.get_all())
    document = Path("/root/Documents/apt-preservation-check.txt")
    document.parent.mkdir(parents=True, exist_ok=True); document.write_text("preserve-me")
    updater = runpy.run_path("/usr/bin/winlux-update-apt", run_name="apt_test")
    updater["install"](Path("/var/cache/winlux/installed.deb"))
    assert document.read_text() == "preserve-me"
    assert run("winlux-apt-probe").strip() == "native-apt-ok", "Winlux replaced a user-installed package"
    protected = subprocess.run(["apt-get", "-y", "remove", "winlux-desktop"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    assert protected.returncode != 0, protected.stdout
    assert run("dpkg-query", "-W", "-f=${Status}", "winlux-desktop").strip() == "install ok installed"
    run("apt-get", "-y", "purge", "winlux-apt-probe")
    assert not Path("/usr/bin/winlux-apt-probe").exists()
    assert not Path("/usr/share/applications/winlux-apt-probe.desktop").exists()
    document.unlink()
assert run("dpkg", "--audit").strip() == ""
print("WINLUX_NATIVE_APT_RESULT=0", flush=True)
