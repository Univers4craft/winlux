#!/usr/bin/env python3
"""Téléchargement des navigateurs (winlux-browser-fetch) sur un miroir local.

Edge (200 Mo) et Chrome (120 Mo) échouaient sur le Wi-Fi d'un portable
(v0.2.18) : curl avait 5 minutes au plus pour tout le paquet. Ce test
vérifie que le paquet n'a plus de durée maximale (seulement un seuil de
débit), qu'un téléchargement interrompu reprend où il s'était arrêté, et
que l'empreinte SHA-256 de l'index est toujours exigée.
"""
import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
FETCH = HERE / "prepare-browser-packages.py"


def check(cond, message):
    if not cond:
        print(f"ÉCHEC : {message}", file=sys.stderr)
        sys.exit(1)


with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    # Faux paquet « firefox » de 3 Mo, rangé comme dans le dépôt de Mozilla.
    pkg = w / "pkg"
    (pkg / "DEBIAN").mkdir(parents=True)
    (pkg / "usr/lib/firefox").mkdir(parents=True)
    (pkg / "usr/lib/firefox/firefox").write_bytes(os.urandom(3 << 20))
    (pkg / "DEBIAN/control").write_text("Package: firefox\nVersion: 150.0~build1\nArchitecture: amd64\n"
                                        "Maintainer: Test <t@example.org>\nDescription: test\n")
    mirror = w / "mirror"
    deb = mirror / "pool/mozilla/firefox_150.0_amd64.deb"
    deb.parent.mkdir(parents=True)
    subprocess.run(["dpkg-deb", "--build", "-Zgzip", "-z0", str(pkg), str(deb)], check=True, capture_output=True)
    data = deb.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    index = mirror / "dists/mozilla/main/binary-amd64/Packages"
    index.parent.mkdir(parents=True)

    def write_index(sha):
        index.write_text(f"Package: firefox\nVersion: 150.0~build1\nArchitecture: amd64\n"
                         f"Filename: pool/mozilla/firefox_150.0_amd64.deb\nSize: {len(data)}\nSHA256: {sha}\n\n")

    write_index(digest)
    # curl réel, précédé d'un relevé de ses options.
    bindir = w / "bin"
    bindir.mkdir()
    log = w / "curl.log"
    real = subprocess.run(["sh", "-c", "command -v curl"], capture_output=True, text=True).stdout.strip()
    (bindir / "curl").write_text(f'#!/bin/sh\nprintf "%s\\n" "$*" >> "{log}"\nexec {real} "$@"\n')
    (bindir / "curl").chmod(0o755)
    out = w / "out"
    out.mkdir()
    env = {**os.environ, "PATH": f"{bindir}:{os.environ['PATH']}", "WINLUX_BROWSER_MIRROR": mirror.as_uri() + "/"}

    def fetch():
        return subprocess.run([sys.executable, "-I", "-B", str(FETCH), "--only", "firefox", str(out)],
                              env=env, capture_output=True, text=True)

    # Téléchargement interrompu à mi-chemin : il reprend au lieu de recommencer.
    partial = out / f".firefox-{digest[:16]}.partial"
    partial.write_bytes(data[: len(data) // 2])
    (out / ".firefox-0000000000000000.partial").write_bytes(b"ancienne version")
    r = fetch()
    check(r.returncode == 0 and "firefox: 150.0~build1 verified" in r.stdout, f"téléchargement : {r.stderr}")
    check((out / "firefox.deb").read_bytes() == data, "paquet reconstitué différent de l'original")
    check((out / "firefox.sha256").read_text().strip() == digest, "empreinte non enregistrée")
    check(not list(out.glob(".firefox-*.partial")), "fichiers partiels laissés")
    calls = log.read_text().splitlines()
    package_call = next((c for c in calls if c.endswith(".partial") and "firefox_150.0_amd64.deb" in c), "")
    check(package_call, f"téléchargement du paquet absent : {calls}")
    check("--max-time" not in package_call, "durée maximale sur le paquet : Edge et Chrome échouent sur un Wi-Fi lent")
    check("--speed-limit" in package_call and "--speed-time" in package_call, "pas de seuil de débit")
    check("--continue-at -" in package_call, "pas de reprise du téléchargement")
    index_call = next(c for c in calls if "Packages" in c)
    check("--max-time" in index_call, "l'index doit garder une durée maximale")

    # Empreinte fausse dans l'index : refusé, et rien de partiel n'est gardé.
    (out / "firefox.deb").unlink()
    write_index("0" * 64)
    r = fetch()
    check(r.returncode != 0 and "checksum mismatch" in r.stderr, "empreinte fausse acceptée")
    check(not (out / "firefox.deb").exists() and not list(out.glob(".firefox-*.partial")), "paquet refusé gardé")
    # Paquets de langue de Firefox (fr, de, es ; architecture « all ») : téléchargés
    # avec lui, vérifiés ; un paquet absent ou altéré n'empêche pas l'installation.
    langs = {}
    # Espagnol : Mozilla ne publie que firefox-l10n-es-es, -es-ar, -es-cl, -es-mx (#432).
    for lang in ("fr", "de", "es-es", "es-ar"):
        lp = w / f"lp-{lang}"
        (lp / "DEBIAN").mkdir(parents=True)
        (lp / "usr/lib/firefox/distribution/extensions").mkdir(parents=True)
        (lp / f"usr/lib/firefox/distribution/extensions/langpack-{lang}@firefox.mozilla.org.xpi").write_text(lang)
        (lp / "DEBIAN/control").write_text(f"Package: firefox-l10n-{lang}\nVersion: 150.0\nArchitecture: all\n"
                                           "Maintainer: Test <t@example.org>\nDescription: test\n")
        lpdeb = mirror / f"pool/mozilla/firefox-l10n-{lang}_150.0_all.deb"
        subprocess.run(["dpkg-deb", "--build", "-Zgzip", str(lp), str(lpdeb)], check=True, capture_output=True)
        langs[lang] = (lpdeb, hashlib.sha256(lpdeb.read_bytes()).hexdigest())
    write_index(digest)
    # Comme le dépôt de Mozilla : les paquets « all » sont dans binary-all/Packages,
    # jamais dans l'index binary-amd64 du navigateur.
    index_all = mirror / "dists/mozilla/main/binary-all/Packages"
    index_all.parent.mkdir(parents=True, exist_ok=True)
    with index_all.open("w") as stream:
        for lang, (lpdeb, sha) in langs.items():
            if lang == "de": sha = "0" * 64
            stream.write(f"Package: firefox-l10n-{lang}\nVersion: 150.0\nArchitecture: all\n"
                         f"Filename: pool/mozilla/{lpdeb.name}\nSHA256: {sha}\n\n")
    check("firefox-l10n" not in index.read_text(), "le miroir de test doit ranger les paquets de langue comme Mozilla")
    r = fetch()
    check(r.returncode == 0, f"paquet de langue altéré bloquant : {r.stderr}")
    check("binary-all/Packages" in log.read_text(), "index binary-all des paquets de langue jamais consulté")
    check((out / "firefox-l10n-fr.deb").read_bytes() == langs["fr"][0].read_bytes(), "paquet de langue fr absent")
    check((out / "firefox-l10n-fr.sha256").read_text().strip() == langs["fr"][1], "empreinte fr absente")
    check(not (out / "firefox-l10n-de.deb").exists() and "firefox-l10n-de: indisponible" in r.stderr, "paquet de langue altéré gardé")
    # Winlux en espagnol d'Espagne (langue du système par défaut ici : LANG du test).
    check((out / "firefox-l10n-es.deb").read_bytes() == langs["es-es"][0].read_bytes(), "espagnol absent (firefox-l10n-es-es)")
    check((out / "firefox-l10n-es.package").read_text().strip() == "firefox-l10n-es-es", "nom réel du paquet espagnol")
    check((out / "firefox-l10n-es.sha256").read_text().strip() == langs["es-es"][1], "empreinte espagnole absente")
    # Système en espagnol d'Argentine : le paquet du pays ; pays inconnu ou autre langue : l'Espagne.
    for locale, expected in (("es_AR.UTF-8", "es-ar"), ("es_PE.UTF-8", "es-es"), ("fr_FR.UTF-8", "es-es")):
        env_locale = {**env, "LANG": locale, "LC_ALL": "", "LC_MESSAGES": ""}
        r = subprocess.run([sys.executable, "-I", "-B", str(FETCH), "--only", "firefox", str(out)],
                           env=env_locale, capture_output=True, text=True)
        check(r.returncode == 0, f"téléchargement en {locale} : {r.stderr}")
        check((out / "firefox-l10n-es.deb").read_bytes() == langs[expected][0].read_bytes(),
              f"paquet espagnol en {locale} : firefox-l10n-{expected} attendu")
    check(not list(out.glob(".l10n-*.partial")), "paquet de langue partiel laissé")

    # Edge, Chrome et Firefox : les vrais dépôts des éditeurs, en HTTPS.
    source = FETCH.read_text()
    for url in ("https://packages.microsoft.com/repos/edge/", "https://dl.google.com/linux/chrome/deb/",
                "https://packages.mozilla.org/apt/"):
        check(url in source, f"dépôt de l'éditeur absent : {url}")
# Navigateurs ajoutés le 11 novembre 2026 (#1716).
import gzip  # noqa: E402
import json  # noqa: E402
import shutil  # noqa: E402
source = FETCH.read_text()
for url in ("https://brave-browser-apt-release.s3.brave.com/", "https://deb.opera.com/opera-stable/",
            "https://deb.opera.com/opera-gx-stable/", "https://repo.vivaldi.com/archive/deb/",
            "https://aus1.torproject.org/torbrowser/update_3/release/download-linux-x86_64.json"):
    check(url in source, f"source officielle absente : {url}")
check('TOR_FINGERPRINT = "EF6E286DDA85EA2A4BA7DE684E2C6E8793298290"' in source, "clé de Tor Browser non épinglée")
with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    mirror = w / "mirror"
    out = w / "out"
    out.mkdir()
    env = {**os.environ, "WINLUX_BROWSER_MIRROR": mirror.as_uri() + "/"}

    def run(*args, extra=None):
        return subprocess.run([sys.executable, "-I", "-B", str(FETCH), *args], env={**env, **(extra or {})},
                              capture_output=True, text=True)

    # Opera GX : index compressé seulement (Packages.gz), nom de paquet « opera-gx » accepté.
    pkg = w / "gx"
    (pkg / "DEBIAN").mkdir(parents=True)
    (pkg / "DEBIAN/control").write_text("Package: opera-gx\nVersion: 120.0\nArchitecture: amd64\n"
                                        "Maintainer: Test <t@example.org>\nDescription: test\n")
    deb = mirror / "pool/non-free/o/opera-gx_120.0_amd64.deb"
    deb.parent.mkdir(parents=True)
    subprocess.run(["dpkg-deb", "--build", str(pkg), str(deb)], check=True, capture_output=True)
    sha = hashlib.sha256(deb.read_bytes()).hexdigest()
    index = mirror / "dists/stable/non-free/binary-amd64/Packages.gz"
    index.parent.mkdir(parents=True)
    index.write_bytes(gzip.compress(f"Package: opera-gx\nVersion: 120.0\nArchitecture: amd64\n"
                                    f"Filename: pool/non-free/o/{deb.name}\nSHA256: {sha}\n\n".encode()))
    r = run("--only", "operagx", str(out))
    check(r.returncode == 0 and (out / "operagx.deb").read_bytes() == deb.read_bytes(), f"Opera GX : {r.stderr}")
    check((out / "operagx.package").read_text().strip() == "opera-gx", "nom réel du paquet d'Opera GX non noté")

    # Tor Browser : archive, liste SHA-256 signée, signature de l'archive.
    if shutil.which("gpg") and shutil.which("gpgv"):
        home = w / "gnupg"
        home.mkdir(mode=0o700)
        genv = {**os.environ, "GNUPGHOME": str(home)}
        gpg = lambda *a: subprocess.run(["gpg", "--batch", "--quiet", "--yes", "--pinentry-mode", "loopback",
                                         "--passphrase", "", *a], env=genv, check=True, capture_output=True)
        gpg("--quick-gen-key", "Tor fixture <tor@example.invalid>", "ed25519", "sign", "never")
        gpg("--quick-gen-key", "Autre <autre@example.invalid>", "ed25519", "sign", "never")
        listing = subprocess.run(["gpg", "--batch", "--with-colons", "--list-keys"], env=genv, capture_output=True, text=True).stdout
        good, other = [l.split(":")[9] for l in listing.splitlines() if l.startswith("fpr:")][:2]
        version = "16.0.1"
        folder = mirror / "torbrowser" / version
        folder.mkdir(parents=True)
        tree = w / "tree/tor-browser/Browser"
        tree.mkdir(parents=True)
        (tree / "start-tor-browser").write_text("#!/bin/sh\n")
        archive = folder / f"tor-browser-linux-x86_64-{version}.tar.xz"
        subprocess.run(["tar", "-cJf", str(archive), "-C", str(w / "tree"), "tor-browser"], check=True)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (folder / "sha256sums-signed-build.txt").write_text(f"{'0' * 64}  autre.tar.xz\n{digest}  {archive.name}\n")
        url = mirror.as_uri() + f"/torbrowser/{version}/{archive.name}"
        (mirror / "download-linux-x86_64.json").write_text(json.dumps({"binary": url, "sig": url + ".asc", "version": version}))
        gpg("--export", "--armor", "--output", str(mirror / "torbrowser.key"), good)

        def sign(key):
            for target in (archive, folder / "sha256sums-signed-build.txt"):
                gpg("--local-user", key, "--armor", "--detach-sign", "--output", str(target) + ".asc", str(target))
        sign(good)
        tor_env = {"WINLUX_BROWSER_TOR_FINGERPRINT": good}
        r = run("--info", "tor", extra=tor_env)
        check(r.returncode == 0 and f"sha256={digest}" in r.stdout and f"version={version}" in r.stdout
              and "signature=tor" in r.stdout, f"--info tor : {r.stdout} {r.stderr}")
        r = run("--only", "tor", str(out), extra=tor_env)
        check(r.returncode == 0 and (out / "tor.tar.xz").read_bytes() == archive.read_bytes(), f"Tor Browser : {r.stderr}")
        for name in ("tor.tar.xz.asc", "tor.keyring", "tor.sha256", "tor.version", "tor.source"):
            check((out / name).is_file(), f"{name} absent")
        check((out / "tor.sha256").read_text().strip() == digest, "SHA-256 de Tor Browser")
        # Clé servie différente de celle épinglée : refusée.
        r = run("--info", "tor", extra={"WINLUX_BROWSER_TOR_FINGERPRINT": other})
        check(r.returncode != 0 and "empreinte inattendue" in r.stderr, "clé non épinglée acceptée")
        # Liste et archive signées par une autre clé : refusées, rien de gardé.
        (out / "tor.tar.xz").unlink()
        sign(other)
        r = run("--only", "tor", str(out), extra=tor_env)
        check(r.returncode != 0 and "signature" in r.stderr and not (out / "tor.tar.xz").exists(), "mauvaise signature acceptée")
        # Liste bien signée mais archive signée par une autre clé : refusée.
        gpg("--local-user", good, "--armor", "--detach-sign", "--output", str(folder / "sha256sums-signed-build.txt.asc"),
            str(folder / "sha256sums-signed-build.txt"))
        r = run("--only", "tor", str(out), extra=tor_env)
        check(r.returncode != 0 and not (out / "tor.tar.xz").exists() and not list(out.glob(".tor-*.partial")),
              "archive mal signée acceptée")
        # Adresse d'archive hors du dossier attendu : refusée.
        (mirror / "download-linux-x86_64.json").write_text(json.dumps({"binary": "https://example.invalid/t.tar.xz",
                                                                        "sig": "https://example.invalid/t.tar.xz.asc", "version": version}))
        r = run("--info", "tor", extra=tor_env)
        check(r.returncode != 0 and "inattendue" in r.stderr, "adresse de Tor Browser non contrôlée")
    else:
        print("gpg/gpgv absents : partie Tor Browser sautée", file=sys.stderr)
print("OK : téléchargement des navigateurs sans durée maximale, reprise, empreinte SHA-256, paquets de langue de Firefox, Opera GX, Tor Browser signé (#1716)")
