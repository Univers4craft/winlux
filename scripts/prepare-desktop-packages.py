#!/usr/bin/env python3
"""Download a coherent native desktop dependency set without installing on host."""
import pathlib
import re
import shutil
import subprocess
import sys

PACKAGES = ["openbox", "xfce4-panel", "xfce4-whiskermenu-plugin", "mousepad", "thunar",
            "xfconf", "xscreensaver", "xscreensaver-gl", "wmctrl", "xdotool",
            "dbus-x11", "adwaita-icon-theme", "libgl1-mesa-dri", "xdg-utils",
            "python3-minimal", "xfce4-genmon-plugin", "alsa-utils", "librsvg2-common",
            "x11-utils", "x11-xserver-utils", "xfdesktop4", "locales", "libglib2.0-bin",
            "libgdk-pixbuf2.0-bin", "desktop-file-utils", "shared-mime-info", "x11-apps",
            "python3-gi", "gir1.2-gtk-3.0", "network-manager", "network-manager-gnome",
            "bluez", "blueman", "brightnessctl", "flatpak", "gvfs", "gvfs-backends",
            "udisks2", "thunar-volman", "tumbler", "ristretto", "gnome-disk-utility",
            "xfce4-settings", "policykit-1-gnome", "wpasupplicant"]

def main():
    work = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "build/desktop-components").resolve()
    work.mkdir(parents=True, exist_ok=True)
    for directory in ["state/lists/partial", "cache/archives/partial", "log"]:
        (work / directory).mkdir(parents=True, exist_ok=True)
    release = dict(line.split("=", 1) for line in pathlib.Path("/etc/os-release").read_text().splitlines() if "=" in line)
    suite = release.get("UBUNTU_CODENAME", release.get("VERSION_CODENAME", "")).strip('"')
    if not re.fullmatch(r"[a-z]+", suite):
        raise RuntimeError("An Ubuntu archive codename is required")
    # Only Ubuntu archives, not unrelated third-party host repositories.
    sources = work / "sources.list"
    sources.write_text("".join(f"deb [signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg arch=amd64] {base} {name} main universe multiverse\n"
                               for base, name in [("https://archive.ubuntu.com/ubuntu", suite),
                                                  ("https://archive.ubuntu.com/ubuntu", suite + "-updates"),
                                                  ("https://security.ubuntu.com/ubuntu", suite + "-security")]))
    options = ["-o", "Debug::NoLocking=1", "-o", f"Dir::State={work / 'state'}",
               "-o", "Dir::State::status=/dev/null", "-o", f"Dir::Cache={work / 'cache'}",
               "-o", f"Dir::Log={work / 'log'}", "-o", "Acquire::Retries=2",
               "-o", f"Dir::Etc::sourcelist={sources}", "-o", "Dir::Etc::sourceparts=-"]
    subprocess.run(["apt-get", *options, "update"], check=True)
    plan = subprocess.check_output(["apt-get", *options, "--simulate", "--no-install-recommends", "install", *PACKAGES], text=True)
    selected = {(name.split(":", 1)[0], version) for name, version in
                re.findall(r"^Inst (\S+) (?:\[[^\]]*\] )?\((\S+)", plan, re.MULTILINE)}
    if not selected: raise RuntimeError("Empty native desktop dependency plan")
    subprocess.run(["apt-get", *options, "--download-only", "--yes",
                    "--no-install-recommends", "install", *PACKAGES], check=True)
    root = work / "root"
    if root.exists():
        if not (work / ".winlux-components").is_file():
            raise RuntimeError("Refusing to replace an unmarked component root")
        shutil.rmtree(root)
    root.mkdir()
    (work / ".winlux-components").touch()
    for package in sorted((work / "cache/archives").glob("*.deb")):
        metadata = subprocess.check_output(["dpkg-deb", "-f", str(package), "Package", "Version"], text=True)
        fields = dict(line.split(": ", 1) for line in metadata.splitlines() if ": " in line)
        if (fields.get("Package"), fields.get("Version")) not in selected: continue
        subprocess.run(["dpkg-deb", "-x", str(package), str(root)], check=True)
    print(f"Native desktop payload ready: {root}")

if __name__ == "__main__":
    main()
