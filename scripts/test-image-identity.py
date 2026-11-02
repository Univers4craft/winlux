#!/usr/bin/python3
"""Identité juridique de l'image : nom Winlux, pas de logo Ubuntu, sources GPL.

Sur un faux système : scripts/winlux-brand-image.sh remplace os-release,
lsb-release, issue et legal (détournés, donc gardés par les mises à jour
APT), retire les logos Ubuntu et les exclut des futures installations, et
pose le logo Prisme comme « distributor-logo ». Vérifie aussi que l'ISO
appelle ce script et list-gpl-sources.py, et que release.yml publie
sources-gpl.tsv (docs/NOM-ET-STRUCTURE-JURIDIQUE.md, section 7, points 2 et 5).
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

project = Path(__file__).resolve().parent.parent
brand = project / "scripts/winlux-brand-image.sh"

with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    for directory in ("etc", "usr/lib", "var/lib/dpkg/info", "usr/share/icons/Yaru/48x48/places",
                      "usr/share/icons/hicolor/48x48/apps", "usr/share/plymouth/themes/ubuntu-text"):
        (root / directory).mkdir(parents=True)
    (root / "var/lib/dpkg/status").write_text("")
    (root / "var/lib/dpkg/diversions").write_text("")
    (root / "usr/lib/os-release").write_text('NAME="Ubuntu"\nPRETTY_NAME="Ubuntu 26.04 LTS"\nID=ubuntu\n'
                                             'VERSION_CODENAME=resolute\nUBUNTU_CODENAME=resolute\n')
    os.symlink("../usr/lib/os-release", root / "etc/os-release")
    (root / "etc/lsb-release").write_text("DISTRIB_ID=Ubuntu\nDISTRIB_DESCRIPTION=\"Ubuntu 26.04 LTS\"\n")
    (root / "etc/issue").write_text("Ubuntu 26.04 LTS \\n \\l\n")
    logos = ["usr/share/icons/Yaru/48x48/places/start-here.png",
             "usr/share/icons/hicolor/48x48/apps/ubuntu-logo-icon.png",
             "usr/share/plymouth/ubuntu-logo.png"]
    for logo in logos:
        (root / logo).write_bytes(b"logo")
    # Deux passages : le script doit pouvoir être relancé sur la même racine.
    for _ in range(2):
        subprocess.run(["sh", str(brand), str(root)], check=True, stdout=subprocess.DEVNULL)

    release = dict(line.split("=", 1) for line in (root / "etc/os-release").read_text().splitlines() if "=" in line)
    assert (root / "etc/os-release").is_symlink(), "lien /etc/os-release perdu"
    assert release["NAME"] == '"Winlux"' and release["ID"] == "winlux", release
    assert "ubuntu" not in release["PRETTY_NAME"].lower(), release
    # Compatibilité des outils qui cherchent la base Ubuntu.
    assert release["UBUNTU_CODENAME"] == "resolute" and "ubuntu" in release["ID_LIKE"], release
    for name in ("etc/lsb-release", "etc/issue", "etc/issue.net"):
        assert "Ubuntu" not in (root / name).read_text(), name
    legal = (root / "etc/legal").read_text()
    assert "construit à partir de paquets Ubuntu" in legal and "sources-gpl.tsv" in legal, legal
    assert "Ubuntu" in (root / "usr/lib/os-release.ubuntu").read_text(), "original d'Ubuntu non conservé"
    if shutil.which("dpkg-divert"):
        diversions = (root / "var/lib/dpkg/diversions").read_text().split("\n")
        for path in ("/usr/lib/os-release", "/etc/lsb-release", "/etc/issue", "/etc/legal"):
            assert path in diversions and path + ".ubuntu" in diversions, path
        assert diversions.count("/usr/lib/os-release") == 1, "détournement en double"
    for logo in logos:
        assert not (root / logo).exists(), logo
    assert not (root / "usr/share/plymouth/themes/ubuntu-text").exists()
    excludes = (root / "etc/dpkg/dpkg.cfg.d/winlux-brand").read_text()
    assert "path-exclude=/usr/share/icons/*/*/*/distributor-logo*" in excludes, excludes
    assert "path-exclude=/usr/share/plymouth/themes/ubuntu-*" in excludes, excludes
    prism = root / "usr/share/icons/hicolor/scalable/apps/distributor-logo.svg"
    assert prism.read_bytes() == (project / "assets/brand/winlux-logo.svg").read_bytes()
    assert (root / "usr/share/icons/hicolor/48x48/apps/distributor-logo.png").is_file()

build = (project / "scripts/build-live-iso.sh").read_text()
assert 'scripts/winlux-brand-image.sh" "$stage"' in build
assert 'list-gpl-sources.py" "$stage" "$stage/usr/share/doc/winlux/sources-gpl.tsv"' in build
release_yml = (project / ".github/workflows/release.yml").read_text()
assert "test -s build/sources-gpl.tsv" in release_yml
assert '.iso.sha256" build/sources-gpl.tsv' in release_yml, "sources-gpl.tsv absent des fichiers publiés"
assert "ni affilié à Microsoft" in release_yml and "--notes-file build/release-notes.md" in release_yml
# Mention de non-affiliation partout où l'on installe Winlux (docs/DEFENSE-DU-NOM.md).
for path, needle in (("scripts/winlux-installer.py", 'tr("independent_notice")'),
                     ("scripts/winlux-first-boot.sh", "ni affilié à Microsoft"),
                     ("scripts/winlux-welcome.py", "neither affiliated with nor endorsed by Microsoft"),
                     ("installation/winlux_usb/gui.py", 'tr("independent")')):
    assert needle in (project / path).read_text(), path
import sys  # noqa: E402
sys.path.insert(0, str(project / "installation"))
from winlux_usb import i18n  # noqa: E402
assert all("Microsoft" in text for text in i18n.T["independent"]) and len(i18n.T["independent"]) == len(i18n.LANGS)
installer = (project / "scripts/winlux-installer.py").read_text()
assert installer.count('"independent_notice":') == 6, "mention absente d'une langue de l'installateur"
# Choix du navigateur : aucun fourni, aucun présélectionné, ordre alphabétique.
assert installer.count('"browser_subtitle":') == 6, "choix du navigateur absent d'une langue"
assert 'BROWSER_CHOICES = [("chrome", "Google Chrome"), ("edge", "Microsoft Edge"), ("firefox", "Mozilla Firefox")]' in installer
assert '"browser": None,' in installer and 'first-boot-browser' in installer
first_boot = (project / "scripts/winlux-first-boot.sh").read_text()
assert "MENU_ARGS='aucun Choisir_plus_tard chrome Google_Chrome edge Microsoft_Edge firefox Mozilla_Firefox'" in first_boot
assert "Navigateur_Firefox on" not in first_boot and "apt-get install -y --no-install-recommends firefox" not in first_boot
assert 'winlux-browsers --default "$choice"' in build and "first-boot-browser.done" in build
sys.path.insert(0, str(project / "scripts"))
import winlux_autoinstall  # noqa: E402
with tempfile.TemporaryDirectory() as temp:
    answers = Path(temp) / "winlux-answers.json"
    for value, ok in (("firefox", True), ("edge", True), ("safari", False)):
        answers.write_text('{"version": 1, "browser": "%s"}' % value)
        try:
            settings, _steps = winlux_autoinstall.load(str(answers))
            assert ok and settings["browser"] == value
        except winlux_autoinstall.AnswerFileError:
            assert not ok, value
browsers_sh = (project / "scripts/winlux-browsers.sh").read_text()
assert "browser=${2:-none}" in browsers_sh and "printf 'none\\n'" in browsers_sh and "edge\\n' ;;" not in browsers_sh
settings_py = (project / "scripts/winlux-settings.py").read_text()
assert 'for name in ("chrome", "edge", "firefox")' in settings_py
guest = (project / "scripts/test-native-session-guest.py").read_text()
assert "/usr/share/doc/winlux/sources-gpl.tsv" in guest and "WINLUX_NATIVE_IMAGE_IDENTITY" in guest

print("test-image-identity : OK (nom Winlux, logos Ubuntu retirés, sources GPL publiées)")
