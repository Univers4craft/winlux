#!/usr/bin/python3
"""Tests du fichier de réponse Winlux (sans GTK ni disque)."""
import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import winlux_autoinstall as auto  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXAMPLE = os.path.join(ROOT, "integration", "autoinstall", "winlux-autoinstall.example.yaml")
failures = 0


def check(name, condition):
    global failures
    print(("ok   " if condition else "FAIL ") + name)
    failures += not condition


def rejects(name, data, fragment):
    try:
        auto.validate(data)
    except auto.AnswerFileError as error:
        check(name, fragment in str(error))
        return
    check(name + " (aucune erreur levée)", False)


with open(EXAMPLE, encoding="utf-8") as handle:
    text = handle.read()
parsed = auto.parse_simple_yaml(text)
settings, steps = auto.validate(parsed)
check("exemple : analyseur YAML intégré", parsed["account"]["username"] == "winlux" and parsed["packages"] == ["vlc", "gimp"])
check("exemple : étapes sautées", steps == {auto.STEP_LANGUAGE, auto.STEP_KEYBOARD, auto.STEP_LICENSE,
                                             auto.STEP_DISK, auto.STEP_ACCOUNT})
check("numéros de pages de l'installateur", (auto.STEP_DISK, auto.STEP_ACCOUNT, auto.STEP_SUMMARY) == (4, 9, 11))
unattended = dict(parsed, unattended=True)
check("unattended : réseau, import, chiffrement, benchmark, profil, migration et résumé sautés",
      {auto.STEP_NETWORK, auto.STEP_IMPORT, auto.STEP_ENCRYPTION, auto.STEP_BENCHMARK, auto.STEP_PROFILE,
       auto.STEP_MIGRATION, auto.STEP_SUMMARY} <= auto.validate(unattended)[1])
check("pages : ordre identique à STEP_KEYS",
      [auto.STEP_KEYS.index(k) for k in ("network", "disk", "import", "encryption", "benchmark", "profile",
                                         "account", "migration", "summary")]
      == [auto.STEP_NETWORK, auto.STEP_DISK, auto.STEP_IMPORT, auto.STEP_ENCRYPTION, auto.STEP_BENCHMARK,
          auto.STEP_PROFILE, auto.STEP_ACCOUNT, auto.STEP_MIGRATION, auto.STEP_SUMMARY])
check("exemple : réglages", settings["timezone"] == "Europe/Paris" and settings["disk_request"] == "auto")
try:
    import yaml
    check("exemple : identique à PyYAML", yaml.safe_load(text) == parsed)
except ImportError:
    print("skip PyYAML absent")

check("YAML : liste en ligne et commentaire", auto.parse_simple_yaml("packages: [a, b]  # x\nname: 'a # b'\n") == {"packages": ["a", "b"], "name": "a # b"})
check("JSON accepté", auto.load_text(json.dumps({"language": "en_US"}), "f.json") == {"language": "en_US"})

partial, partial_steps = auto.validate({"language": "de_DE"})
check("langue seule : clavier déduit", partial["keyboard"] == "de" and partial_steps == {0, 1})
check("compte sans mot de passe : page affichée", auto.STEP_ACCOUNT not in auto.validate({"account": {"username": "abc"}})[1])
check("licence false : page affichée", 2 not in auto.validate({"license_accepted": False})[1])

rejects("compte en ligne refusé", {"account": {"username": "abc", "microsoft_account": "x@y"}}, "compte local uniquement")
rejects("avatar refusé", {"account": {"avatar": "a.png"}}, "avatar")
rejects("clé inconnue refusée", {"langage": "fr_FR"}, "langage")
rejects("unattended incomplet", {"unattended": True, "language": "fr_FR"}, "partitioning.disk")
rejects("mode de partitionnement", {"partitioning": {"mode": "manual"}}, "full")
rejects("paquet invalide", {"packages": ["vlc; rm -rf /"]}, "paquet invalide")
rejects("utilisateur root", {"account": {"username": "root"}}, "username")
rejects("fuseau invalide", {"timezone": "../../etc/passwd"}, "timezone")
rejects("empreinte invalide", {"account": {"password_hash": "clair"}}, "password_hash")

disks = [{"path": "/dev/vda"}, {"path": "/dev/vdb"}]
check("disque auto ambigu", auto.resolve_disk("auto", disks) is None)
check("disque auto unique", auto.resolve_disk("auto", disks[:1]) == disks[0])
check("disque nommé", auto.resolve_disk("/dev/vdb", disks) == disks[1])

with tempfile.TemporaryDirectory() as tmp:
    answer = os.path.join(tmp, "a.yaml")
    open(answer, "w").write("language: fr_FR\n")
    cmdline = os.path.join(tmp, "cmdline")
    open(cmdline, "w").write(f"quiet winlux.autoinstall={answer}\n")
    check("détection par ligne de commande noyau", auto.find_answer_file(cmdline, {}) == answer)
    check("détection par variable", auto.find_answer_file(cmdline, {"WINLUX_AUTOINSTALL": answer}) == answer)

sys.exit(1 if failures else 0)
