#!/usr/bin/env python3
"""Notices et licences du paquet winlux-desktop (#33) : NOTICES couvre chaque
bibliothèque Rust compilée dans winlux, et scripts/stage-notices.py pose le
fichier copyright, LICENSE, NOTICES et les textes de licence de chacune."""
import runpy
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
tool = runpy.run_path(str(project / "scripts/stage-notices.py"))
failures = []


def ok(condition, message):
    print(("ok   " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


meta = tool["metadata"]()
packages = tool["crates"](meta)
names = {p["name"] for p in packages}
ok({"libc", "x11rb", "png", "rustix"} <= names, f"bibliothèques de winlux trouvées ({len(packages)})")
ok("winlux" not in names, "le paquet winlux lui-même n'est pas une bibliothèque tierce")
notices = (project / "NOTICES").read_text(encoding="utf-8")
ok(tool["missing_from_notices"](notices, packages) == [], "NOTICES cite chaque bibliothèque avec sa licence")

# Le contrôle repère un oubli et une licence fausse.
fake = [{"name": "bidule", "version": "1.0.0", "license": "MIT"},
        {"name": "libc", "version": "0.2.0", "license": "Zlib"}]
problems = tool["missing_from_notices"](notices, fake)
ok(len(problems) == 2 and "bidule" in problems[0] + problems[1], f"oubli et licence fausse signalés ({problems})")

with tempfile.TemporaryDirectory() as temp:
    doc = tool["stage"](Path(temp), meta)
    ok((doc / "LICENSE").read_text() == (project / "LICENSE").read_text(), "LICENSE (GPL-3.0) livré")
    ok((doc / "NOTICES").read_text() == notices, "NOTICES livré")
    copyright = (doc / "copyright").read_text(encoding="utf-8")
    ok(copyright.startswith("Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"),
       "copyright au format Debian")
    ok("License: GPL-3.0-or-later" in copyright, "copyright : Winlux sous GPL-3.0-or-later")
    for package in packages:
        folder = doc / "crates" / f"{package['name']}-{package['version']}"
        texts = list(folder.iterdir()) if folder.is_dir() else []
        ok(bool(texts), f"{package['name']} {package['version']} : textes de licence livrés ({[t.name for t in texts]})")
        ok(f"crates/{package['name']}-{package['version']}/*" in copyright,
           f"{package['name']} : paragraphe du fichier copyright")
    # Chaque paragraphe DEP-5 a Files, Copyright et License.
    paragraphs = [p for p in copyright.split("\n\n") if p.startswith("Files:")]
    ok(all("\nCopyright:" in p and "\nLicense:" in p for p in paragraphs),
       f"{len(paragraphs)} paragraphes Files complets")

apt = (project / "scripts/stage-apt-root.py").read_text(encoding="utf-8")
ok('"usr/share/doc/winlux-desktop/"' in apt, "le paquet winlux-desktop emporte ses notices")
iso = (project / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
ok(iso.index("stage-notices.py") < iso.index("stage-apt-root.py"),
   "l'ISO pose les notices avant de construire le paquet")

if failures:
    raise SystemExit(f"\n{len(failures)} échec(s)")
print("\nNotices et licences : tout est bon")
