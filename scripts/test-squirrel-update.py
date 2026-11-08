#!/usr/bin/python3 -I
"""Tests de winlux-squirrel-update (#1190), sans réseau : sources en file://,
confirmation et winlux remplacés par de petits scripts."""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

TOOL = Path(__file__).resolve().parent / "winlux-squirrel-update.py"
failures = 0


def ok(cond, label):
    global failures
    print(("ok   " if cond else "ÉCHEC ") + label)
    if not cond:
        failures += 1


def nupkg(path, version, files, title="Claude", authors="Anthropic, PBC"):
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("AnthropicClaude.nuspec",
                   f"<package><metadata><id>AnthropicClaude</id><version>{version}</version>"
                   f"<title>{title}</title><authors>{authors}</authors></metadata></package>")
        for name, data in files.items():
            z.writestr(name, data)
    data = path.read_bytes()
    return f"{hashlib.sha1(data).hexdigest().upper()} {path.name} {len(data)}"


def script(path, body):
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(0o755)


with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    tools, server, root = tmp / "outils", tmp / "serveur", tmp / "AnthropicClaude"
    for d in (tools, server, root / "packages", root / "app-0.14.10"):
        d.mkdir(parents=True)
    answer = tools / "reponse"
    script(tools / "confirm", f'echo "$@" > {tools}/question\nexit $(cat {answer})\n')
    script(tools / "winlux", f'echo "$@" >> {tools}/lances\n')
    env = {**os.environ, "WINLUX_SQUIRREL_CONFIRM": str(tools / "confirm"),
           "WINLUX_BIN": str(tools / "winlux"), "WINLUX_LANG": "fr"}

    def run(*args):
        r = subprocess.run([sys.executable, "-I", str(TOOL), str(root), *args],
                           capture_output=True, text=True, env=env, timeout=60)
        return r.returncode, r.stdout, r.stderr

    url = server.as_uri()
    exe = b"MZ" + "SquirrelAwareVersion".encode("utf-16-le")
    local = nupkg(root / "packages" / "AnthropicClaude-0.14.10-full.nupkg", "0.14.10",
                  {"lib/net45/claude.exe": exe})
    (root / "packages" / "RELEASES").write_text(local + "\n")
    (root / "app-0.14.10" / "claude.exe").write_bytes(exe)
    (server / "RELEASES").write_text("﻿" + local + "\n")

    code, out, _ = run("--checkForUpdate", url)
    info = json.loads(out.strip().splitlines()[-1])
    ok(code == 0 and info["currentVersion"] == "0.14.10" and info["releasesToApply"] == [],
       "pas de nouvelle version : releasesToApply vide")

    new = nupkg(server / "AnthropicClaude-0.14.11-full.nupkg", "0.14.11",
                {"lib/net45/claude.exe": exe, "lib/net45/resources/app.asar": b"asar"})
    delta = "0" * 40 + " AnthropicClaude-0.14.11-delta.nupkg 10"
    (server / "RELEASES").write_text("\n".join([local, delta, new]) + "\n")
    code, out, _ = run("--checkForUpdate=" + url)
    info = json.loads(out.strip().splitlines()[-1])
    ok(code == 0 and info["futureVersion"] == "0.14.11"
       and [r["version"] for r in info["releasesToApply"]] == ["0.14.11"]
       and not info["releasesToApply"][0]["isDelta"],
       "nouvelle version annoncée, paquet complet seulement")
    ok(not (root / "packages" / "AnthropicClaude-0.14.11-full.nupkg").exists(),
       "--checkForUpdate ne télécharge aucun paquet")

    answer.write_text("1")
    code, _, _ = run("--update", url)
    question = (tools / "question").read_text()
    ok(code != 0 and not (root / "app-0.14.11").exists(),
       "refus : rien n'est téléchargé ni installé")
    ok("Claude" in question and "0.14.11" in question and "Anthropic, PBC" in question
       and url in question and "Annuler" in question,
       "la confirmation nomme produit, version, éditeur, source, avec Annuler")
    code, out, _ = run("--checkForUpdate", url)
    ok(json.loads(out.strip().splitlines()[-1])["releasesToApply"] == [],
       "une version refusée n'est plus proposée")

    (root / "packages" / ".winlux-refus-0.14.11").unlink()
    answer.write_text("0")
    code, out, err = run("--update", url)
    ok(code == 0 and (root / "app-0.14.11" / "resources" / "app.asar").read_bytes() == b"asar",
       "mise à jour acceptée : app-0.14.11 installé")
    ok("0.14.11-full.nupkg" in (root / "packages" / "RELEASES").read_text()
       and "SHA-256" in (root / "SquirrelSetup.log").read_text(),
       "paquet inscrit dans RELEASES, SHA-256 journalisé")
    ok("--squirrel-updated 0.14.11" in (tools / "lances").read_text(),
       "crochet --squirrel-updated lancé")
    ok((root / "app-0.14.10" / "claude.exe").is_file(), "l'ancienne version reste en place")

    (tools / "lances").unlink()
    code, _, _ = run("--processStartAndWait", "claude.exe", "-a", "--un --deux")
    ok(code == 0 and str(root / "app-0.14.11" / "claude.exe") + " --un --deux"
       in (tools / "lances").read_text(), "--processStartAndWait lance la version la plus récente")

    bad = server / "AnthropicClaude-0.14.12-full.nupkg"
    line = nupkg(bad, "0.14.12", {"lib/net45/claude.exe": exe})
    data = bytearray(bad.read_bytes())
    data[-30] ^= 0xFF  # le paquet servi n'est plus celui que RELEASES décrit
    bad.write_bytes(bytes(data))
    (server / "RELEASES").write_text(line + "\n")
    code, _, _ = run("--update", url)
    ok(code != 0 and not (root / "app-0.14.12").exists()
       and not (root / "packages" / bad.name).exists(), "SHA-1 faux : paquet refusé et effacé")

    evil = server / "AnthropicClaude-0.14.13-full.nupkg"
    (server / "RELEASES").write_text(nupkg(evil, "0.14.13", {"lib/net45/../../../evade": b"x"}) + "\n")
    code, _, _ = run("--update", url)
    ok(code != 0 and not (tmp / "evade").exists() and not (root / "app-0.14.13").exists(),
       "chemin qui sort du dossier : extraction refusée")

    code, _, err = run("--checkForUpdate", "http://exemple.invalid/maj")
    ok(code != 0 and "https" in err, "source http refusée")
    code, _, err = run("--createShortcut", "claude.exe")
    ok(code != 0 and "non prise en charge" in err, "commande inconnue : erreur claire")

sys.exit(1 if failures else 0)
