#!/usr/bin/python3
"""Offline checks: no native installation marker, network or package writes."""
from pathlib import Path
import runpy
import tempfile

directory = Path(__file__).resolve().parent
updater = runpy.run_path(str(directory / "winlux-update-apt.py"), run_name="apt_metadata_test")
builder = runpy.run_path(str(directory / "stage-apt-root.py"), run_name="apt_payload_test")
valid = ("format=3\nbackend=apt\nbase=ubuntu-resolute\nversion=v0.3.0\n"
         "asset=winlux-desktop-v0.3.0.deb\nsha256=" + "a" * 64 + "\n"
         "download_bytes=123\nunpacked_kib=456\n")
assert updater["manifest"](valid)["version"] == "v0.3.0"
for invalid in [valid + "format=3\n", valid.replace("format=3", "format=2"),
                valid.replace("ubuntu-resolute", "other-base"), valid.replace("asset=winlux", "asset=../winlux"),
                valid.replace("download_bytes=123", "download_bytes=-1"), valid.replace("a"*64, "not-a-hash")]:
    try: updater["manifest"](invalid)
    except ValueError: pass
    else: raise AssertionError("Incompatible or malformed manifest accepted")
for path in ("usr/bin/telnet", "usr/lib/x86_64-linux-gnu/libc.so.6", "usr/lib/modules/old/driver.ko", "usr/share/winlux/kernel-version"):
    assert not builder["winlux_path"](path), path
for path in ("usr/bin/winlux", "usr/share/winlux/native/config/panel.xml", "usr/share/applications/winlux-settings.desktop",
             "usr/lib/winlux/python/winlux_screen.py"):
    assert builder["winlux_path"](path), path
with tempfile.TemporaryDirectory(prefix="apt-cache-check.") as temporary:
    root = Path(temporary); source = root / "source"; target = root / "installed.deb"
    source.write_bytes(b"new package"); target.write_bytes(b"previous package")
    updater["cache_copy"](source, target)
    assert target.read_bytes() == b"new package"
    assert not list(root.glob(".package-*"))
print("OK: native manifest compatibility, package ownership boundaries and atomic cache replacement")

# Mise à jour sans manifeste publié : repli sur l'API GitHub des releases.
import json, subprocess
scope = updater["release_metadata"].__globals__
manifest_url, api_url = scope["MANIFEST_URL"], scope["RELEASE_API"]
release = {"tag_name": "v0.3.1", "assets": [
    {"name": "winlux-live-v0.3.1.iso", "size": 9},
    {"name": "winlux-desktop-v0.3.1.deb", "size": 4096, "digest": "sha256:" + "b" * 64}]}
def fake(files, codes={}):
    def download(url, destination, accept=None):
        if url in codes: raise subprocess.CalledProcessError(codes[url], "curl")
        if url not in files: raise subprocess.CalledProcessError(22, "curl")
        Path(destination).write_text(files[url])
    return download
def failure(message):
    try: scope["release_metadata"](work)
    except RuntimeError as error: assert message in str(error), error
    else: raise AssertionError("expected failure: " + message)
with tempfile.TemporaryDirectory(prefix="apt-fallback-check.") as temporary:
    work = Path(temporary)
    scope["download"] = fake({manifest_url: valid})
    assert scope["release_metadata"](work)[0]["version"] == "v0.3.0"
    scope["download"] = fake({api_url: json.dumps(release)})
    data = scope["release_metadata"](work)[0]
    assert (data["version"], data["asset"], data["sha256"], data["download_bytes"]) == ("v0.3.1", "winlux-desktop-v0.3.1.deb", "b" * 64, "4096"), data
    assert int(data["unpacked_kib"]) >= 4096 * 3 // 1024 and (work / "manifest").read_text().count("format=3") == 1
    del release["assets"][1]["digest"]
    sums = "https://github.com/Univers4craft/winlux/releases/download/v0.3.1/winlux-desktop-v0.3.1.deb.sha256"
    scope["download"] = fake({api_url: json.dumps(release), sums: "c" * 64 + "  winlux-desktop-v0.3.1.deb\n"})
    assert scope["release_metadata"](work)[0]["sha256"] == "c" * 64
    scope["download"] = fake({api_url: json.dumps(release)})
    failure("empreinte SHA-256")
    scope["download"] = fake({api_url: json.dumps({"tag_name": "v0.3.2", "assets": []})})
    failure("ni manifeste ni paquet")
    scope["download"] = fake({}, {manifest_url: 6})
    failure("DNS")
    scope["download"] = fake({}, {api_url: 28})
    failure("délai dépassé")
print("OK: update metadata falls back to the GitHub release when the manifest is missing")

# APT refuse un fichier local qui ne finit pas par .deb (« Unsupported file »).
with tempfile.TemporaryDirectory(prefix="apt-suffix-check.") as temporary:
    work = Path(temporary); tree = work / "pkg"; (tree / "DEBIAN").mkdir(parents=True)
    (tree / "DEBIAN/control").write_text("Package: winlux-desktop\nVersion: 0.3.1\nArchitecture: all\n"
                                         "Maintainer: Winlux <winlux@localhost>\nDescription: test\n")
    download = work / ("download-" + "d" * 64 + ".part")
    subprocess.run(["dpkg-deb", "--root-owner-group", "-Zgzip", "--build", str(tree), str(download)], check=True, capture_output=True)
    seen = []
    real_run = subprocess.run
    def fake_run(command, **kwargs):
        if command[0] == "apt-get":
            seen.append(command[-1]); assert Path(command[-1]).is_file() and command[-1].endswith(".deb"), command
            return subprocess.CompletedProcess(command, 0)
        return real_run(command, **kwargs)
    scope["subprocess"].run = fake_run
    try: updater["install"](download, version="v0.3.1")
    finally: scope["subprocess"].run = real_run
    assert seen and download.is_file() and not Path(seen[0]).exists(), seen
print("OK: downloaded update handed to APT under a .deb name")
