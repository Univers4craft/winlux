#!/usr/bin/env python3
"""Mises à jour signées (#31) : Ed25519 maison conforme à la RFC 8032,
signature du manifeste en CI (sign-update-manifest.sh) et refus côté client
(winlux-update-apt) d'un manifeste non signé ou falsifié dès que la clé
publique est dans l'image."""
import os
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
tool_path = project / "scripts/winlux-signature.py"
tool = runpy.run_path(str(tool_path))
failures = []


def ok(condition, message):
    print(("ok   " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


# RFC 8032, section 7.1, tests 1 à 3.
VECTORS = [
    ("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
     "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a", "",
     "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"),
    ("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
     "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c", "72",
     "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"),
    ("c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
     "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025", "af82",
     "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a"),
]
for number, (seed, public, message, signature) in enumerate(VECTORS, 1):
    seed, public, message, signature = map(bytes.fromhex, (seed, public, message, signature))
    ok(tool["public_key"](seed) == public, f"RFC 8032 test {number} : clé publique")
    ok(tool["sign"](seed, message) == signature, f"RFC 8032 test {number} : signature")
    ok(tool["verify"](public, message, signature), f"RFC 8032 test {number} : vérification")
    ok(not tool["verify"](public, message + b"!", signature), f"RFC 8032 test {number} : message modifié refusé")
    bad = bytearray(signature); bad[5] ^= 1
    ok(not tool["verify"](public, message, bytes(bad)), f"RFC 8032 test {number} : signature modifiée refusée")
ok(not tool["verify"](b"\0" * 31, b"", b"\0" * 64), "clé de mauvaise taille refusée")

MANIFEST = ("format=3\nbackend=apt\nbase=ubuntu-resolute\nversion=v0.3.0\nscope=system\naction=reboot\n"
            "asset=winlux-desktop-v0.3.0.deb\nsha256=" + "a" * 64 + "\ndownload_bytes=10\nunpacked_kib=1\n")

with tempfile.TemporaryDirectory() as temp:
    temp = Path(temp)
    seed_file = temp / "graine"
    created = subprocess.run([sys.executable, "-B", str(tool_path), "new", str(seed_file)], capture_output=True, text=True)
    ok(created.returncode == 0 and (seed_file.stat().st_mode & 0o777) == 0o600, "graine créée en 0600")
    public_file = temp / "update-signing.pub"
    public_file.write_text(created.stdout)
    seed_hex = seed_file.read_text().strip()

    manifest = temp / "winlux-update-manifest.txt"
    manifest.write_text(MANIFEST)
    script = project / "scripts/sign-update-manifest.sh"
    env = {k: v for k, v in os.environ.items() if k != "WINLUX_UPDATE_SIGNING_KEY"}
    unsigned = subprocess.run(["sh", str(script), str(manifest)], capture_output=True, text=True, env=env)
    ok(unsigned.returncode == 0 and not Path(str(manifest) + ".sig").exists(), "sans secret : avertissement, pas de signature")
    required = subprocess.run(["sh", str(script), str(manifest), "--obligatoire"], capture_output=True, text=True, env=env)
    ok(required.returncode == 1, "sans secret mais signature obligatoire : échec")
    signed = subprocess.run(["sh", str(script), str(manifest)], capture_output=True, text=True,
                            env=dict(env, WINLUX_UPDATE_SIGNING_KEY=seed_hex))
    signature = Path(str(manifest) + ".sig")
    ok(signed.returncode == 0 and signature.is_file(), f"avec le secret : manifeste signé ({signed.stderr.strip()})")
    check = subprocess.run([sys.executable, "-B", str(tool_path), "verify", str(public_file), str(manifest), str(signature)])
    ok(check.returncode == 0, "signature du manifeste vérifiée par la clé publique")

    # Client : manifeste local, comme le chemin de test avant publication.
    def client(public, manifest_text, signature_text):
        folder = Path(tempfile.mkdtemp(dir=temp))
        local = folder / "winlux-update-manifest.txt"
        local.write_text(manifest_text)
        if signature_text is not None:
            (folder / "winlux-update-manifest.txt.sig").write_text(signature_text)
        work = folder / "work"; work.mkdir()
        os.environ["WINLUX_UPDATE_MANIFEST"] = str(local)
        os.environ["WINLUX_UPDATE_PUBLIC_KEY"] = str(public)
        module = runpy.run_path(str(project / "scripts/winlux-update-apt.py"))
        try:
            data, _ = module["release_metadata"](work)
            return data["version"]
        except RuntimeError as error:
            return "refus : " + str(error)

    good = signature.read_text()
    ok(client(public_file, MANIFEST, good) == "v0.3.0", "client : manifeste signé accepté")
    forged = client(public_file, MANIFEST.replace("a" * 64, "b" * 64), good)
    ok(forged.startswith("refus") and "invalide" in forged, f"client : empreinte du paquet modifiée refusée ({forged})")
    missing = client(public_file, MANIFEST, None)
    ok(missing.startswith("refus") and "pas signé" in missing, f"client : manifeste sans signature refusé ({missing})")
    other = temp / "autre.pub"
    other.write_text(tool["public_key"](bytes(32)).hex() + "\n")
    ok(client(other, MANIFEST, good).startswith("refus"), "client : signature d'une autre clé refusée")
    ok(client(temp / "absente.pub", MANIFEST, None) == "v0.3.0", "client sans clé publique : comportement d'avant (pas de vérification)")

stage = (project / "scripts/stage-desktop-components.sh").read_text()
ok("winlux-signature" in stage and "update-signing.pub" in stage, "l'image embarque winlux-signature et la clé publique")
release = (project / ".github/workflows/release.yml").read_text()
ok(release.count("sign-update-manifest.sh") >= 2 and "winlux-update-manifest.txt.sig" in release,
   "release.yml signe et publie le manifeste")

if failures:
    raise SystemExit(f"\n{len(failures)} échec(s)")
print("\nMises à jour signées : tout est bon")
