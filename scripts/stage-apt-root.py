#!/usr/bin/env python3
"""Merge Winlux-owned additions into a configured native Ubuntu root.

Distribution files keep their dpkg ownership and bytes. Winlux additions are
installed as a real protected Debian package, never as a synthetic status list.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import runpy
import subprocess
import tarfile
import tempfile

def canonical(path):
    parts = path.lstrip("./").split("/")
    if parts[0] in ("bin", "sbin", "lib", "lib64"): parts.insert(0, "usr")
    return "/".join(parts)

def owned_paths(root):
    return {canonical(line) for inventory in (root / "var/lib/dpkg/info").glob("*.list")
            for line in inventory.read_text().splitlines() if line not in ("/", "/.")}

def winlux_path(relative):
    if relative == "usr/share/winlux/kernel-version": return False
    return (relative.startswith(("usr/bin/winlux", "usr/share/winlux/", "opt/winlux/",
                                "usr/share/themes/Winlux/", "usr/share/xfce4/helpers/winlux"))
            or relative.startswith("usr/share/applications/winlux")
            or relative == "usr/share/mime/packages/winlux.xml"
            or relative == "usr/share/dbus-1/services/org.freedesktop.Notifications.service"
            or relative == "usr/lib/NetworkManager/conf.d/90-winlux.conf"
            or relative == "usr/libexec/xscreensaver/xscreensaver-gfx"
            or relative.startswith("usr/share/icons/") and Path(relative).name.lower().startswith("winlux"))

def build(stage, base, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    version = (stage / "usr/share/winlux/version").read_text().strip().removeprefix("v")
    subprocess.run(["dpkg", "--validate-version", version], check=True)
    with tempfile.TemporaryDirectory(prefix="native-apt-", dir=stage.parent) as temporary:
        work = Path(temporary); native = work / "root"; native.mkdir()
        with tarfile.open(base) as archive:
            special = {canonical(item.name): [item.uid, item.gid, item.mode]
                       for item in archive.getmembers()
                       if (item.isfile() or item.isdir()) and (item.uid or item.gid or item.mode & 0o7022)}
            archive.extractall(native, filter="tar")
        status = native / "var/lib/dpkg/status"
        if not status.is_file() or not (native / "usr/share/winlux/apt-base").is_file():
            raise ValueError("A configured native APT base is required")
        owned = owned_paths(native)
        kernel = work / "kernel"; (kernel / "DEBIAN").mkdir(parents=True)
        for relative in ("boot/vmlinuz", "lib/modules", "usr/share/winlux/kernel-version"):
            source = stage / relative; target = kernel / canonical(relative)
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir(): shutil.copytree(source, target, symlinks=True)
            else: shutil.copy2(source, target)
        release = (stage / "usr/share/winlux/kernel-version").read_text().strip()
        subprocess.run(["dpkg", "--validate-version", release], check=True)
        (kernel / "DEBIAN/control").write_text(
            f"Package: winlux-kernel\nVersion: {release}\nArchitecture: amd64\n"
            "Maintainer: Winlux <winlux@localhost>\nEssential: yes\nProtected: yes\n"
            "Depends: kmod\nDescription: Winlux factory kernel and matching drivers\n")
        kernel_deb = output.with_name("winlux-kernel.deb")
        subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(kernel), str(kernel_deb)], check=True)
        package = work / "package"; package.mkdir()
        # /etc and profile seeds are initial-install defaults, never an update payload.
        for directory in ("root", "etc/winlux", "etc/binfmt.d", "boot", "lib/modules"):
            source = stage / directory
            if source.exists():
                destination = native / canonical(directory)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copytree(source, destination, dirs_exist_ok=True, symlinks=True)
        # Native distribution tools and libraries always win over host copies.
        for directory in ("usr", "bin", "sbin", "lib", "lib64", "opt/winlux"):
            source = stage / directory
            if not source.is_dir() or source.is_symlink(): continue
            for current, folders, files in os.walk(source, followlinks=False):
                links = [name for name in folders if (Path(current) / name).is_symlink()]
                folders[:] = [name for name in folders if name not in links]
                for name in files + links:
                    path = Path(current) / name; relative = canonical(str(path.relative_to(stage)))
                    if not winlux_path(relative): continue
                    if relative == "usr/libexec/xscreensaver/xscreensaver-gfx.real": continue
                    if relative in owned and relative != "usr/libexec/xscreensaver/xscreensaver-gfx": continue
                    # Shared caches must be regenerated by native tools.
                    if relative.endswith((".cache", "gschemas.compiled")): continue
                    target = package / relative; target.parent.mkdir(parents=True, exist_ok=True)
                    if target.exists() or target.is_symlink(): continue
                    if path.is_symlink(): target.symlink_to(os.readlink(path))
                    else: shutil.copy2(path, target)
        shutil.copy2(stage / "init", package / "init")
        # APT installation/removal regenerates the existing XDG application catalogue.
        hooks = package / "etc/apt/apt.conf.d"; hooks.mkdir(parents=True)
        (hooks / "90winlux-desktop").write_text(
            'DPkg::Post-Invoke { "if test -x /usr/bin/winlux-applications; then /usr/bin/winlux-applications refresh; fi"; };\n')
        metadata = package / "DEBIAN"; metadata.mkdir()
        installed_kib = (sum(path.lstat().st_size for path in package.rglob("*") if path.is_file() and not path.is_symlink()) + 1023) // 1024
        (metadata / "control").write_text(
            f"Package: winlux-desktop\nVersion: {version}\nArchitecture: amd64\n"
            f"Installed-Size: {installed_kib}\n"
            "Maintainer: Winlux <winlux@localhost>\nSection: metapackages\nPriority: required\n"
            "Essential: yes\nProtected: yes\n"
            "Depends: " + ", ".join(runpy.run_path(str(Path(__file__).with_name("prepare-apt-base.py")))["DESKTOP_REQUIRED"]) + "\n"
            "Description: Winlux desktop and runtime integration on native APT\n")
        (metadata / "preinst").write_text('#!/bin/sh\nset -e\n'
            'dpkg-divert --root="${DPKG_ROOT:-/}" --package winlux-desktop --add --rename '
            '--divert /usr/libexec/xscreensaver/xscreensaver-gfx.distribution '
            '/usr/libexec/xscreensaver/xscreensaver-gfx\n')
        (metadata / "preinst").chmod(0o755)
        (metadata / "postinst").write_text('#!/bin/sh\nset -e\n'
            'test -z "${DPKG_ROOT:-}" || exit 0\n'
            'update-desktop-database /usr/share/applications\n'
            'update-mime-database /usr/share/mime\n')
        (metadata / "postinst").chmod(0o755)
        (metadata / "conffiles").write_text("/etc/apt/apt.conf.d/90winlux-desktop\n")
        output.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(package), str(output)], check=True)
        # Our scripts explicitly honour DPKG_ROOT and do not execute programs from
        # the target on the build host. Base maintainer scripts ran only in the VM.
        subprocess.run(["fakeroot", "dpkg", "--root=" + str(native), "--no-triggers",
                        "--log=" + str(native / "var/log/dpkg.log"),
                        "--force-not-root", "--force-script-chrootless", "--install", str(kernel_deb), str(output)], check=True)
        cache = native / "var/cache/winlux"; cache.mkdir(parents=True, exist_ok=True)
        shutil.copy2(output, cache / "installed.deb")
        for directory in ("dev", "proc", "sys", "run", "tmp"):
            (native / directory).mkdir(exist_ok=True)
        (native / "tmp").chmod(0o1777)
        # A native base must never be updated by the legacy file-tree installer.
        (native / "usr/share/winlux/update-backend").write_text("apt\n")
        # Image creation is unprivileged and normalizes ownership. Restore the
        # base's service accounts and special modes once, before starting APT
        # or any daemon. Never repeat this over later user-installed packages.
        (native / "usr/share/winlux/apt-base-permissions.json").write_text(json.dumps(special))
        previous = stage.with_name(stage.name + ".before-apt")
        if previous.exists(): raise ValueError(f"Preserved stage already exists: {previous}")
        stage.rename(previous)
        shutil.move(native, stage)
    print(f"Native APT stage: {stage}; package: {output}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", type=Path); parser.add_argument("base", type=Path)
    parser.add_argument("--package", type=Path, default=Path("build/winlux-desktop.deb"))
    args = parser.parse_args()
    build(args.stage.resolve(), args.base.resolve(), args.package.resolve())
