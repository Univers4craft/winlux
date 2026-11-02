#!/usr/bin/python3
"""Paramètres > Alimentation selon le matériel réellement détecté.

Sans batterie, la valeur « Batterie » et les délais « sur batterie »
disparaissent (jamais grisés) et la page s'appelle « Alimentation » ; sans
capot ni rétroéclairage, leurs réglages disparaissent aussi. Puis : plus de
comparaison à « Windows 11 » dans la visite guidée ni dans l'aide de
winlux-accounts, et le chemin de l'Explorateur en noms localisés.
"""
from pathlib import Path
import os
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="power_test")
hardware, items = settings["power_hardware"], settings["power_page_items"]
fields, actions = settings["FIELDS"][23], settings["ACTIONS"][23]

with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    check(hardware(root) == (False, False, False), "machine vide : rien détecté")
    ac = root / "sys/class/power_supply/AC"
    ac.mkdir(parents=True)
    (ac / "type").write_text("Mains\n")
    (ac / "online").write_text("1\n")
    check(hardware(root)[0] is False, "secteur seul : pas de batterie")
    bat = root / "sys/class/power_supply/BAT0"
    bat.mkdir(parents=True)
    (bat / "type").write_text("Battery\n")
    (bat / "present").write_text("0\n")
    (bat / "capacity").write_text("0\n")
    check(hardware(root)[0] is False, "emplacement de batterie vide : pas de batterie")
    (bat / "present").write_text("1\n")
    check(hardware(root)[0] is True, "batterie présente détectée")
    (root / "proc/acpi/button/lid/LID0").mkdir(parents=True)
    (root / "proc/acpi/button/lid/LID0/state").write_text("state: open\n")
    (root / "sys/class/backlight/intel_backlight").mkdir(parents=True)
    (root / "sys/class/backlight/intel_backlight/brightness").write_text("50\n")
    check(hardware(root) == (True, True, True), "portable : batterie, capot, rétroéclairage")

desktop_fields, desktop_actions = items(fields, actions, (False, False, False))
keys = [key for _, key in desktop_fields] + [key for _, key in desktop_actions]
check("battery" not in keys, "« Batterie » affichée sans batterie")
check(not any(key.endswith("-dc") for key in keys), f"délais sur batterie sans batterie : {keys}")
check("power_lid" not in keys and "power-lid" not in keys, "capot affiché sans capot")
check(not any("brightness" in key for key in keys), "luminosité affichée sans rétroéclairage")
check("power-screen-ac" in keys and "power-sleep-ac" in keys and "power-suspend" in keys, "réglages secteur gardés")
laptop_fields, laptop_actions = items(fields, actions, (True, True, True))
check(laptop_fields == fields and laptop_actions == actions, "portable : tout est affiché")
check(settings["_TR"]["Alimentation"] == ("Power", "Energía", "Energie"), "« Alimentation » en quatre langues")
source = (HERE / "winlux-settings.py").read_text()
check('if not power_hardware()[0]: PAGES[23] = _t("Alimentation")' in source, "titre sans batterie")
check("power_page_items(fields, [], power_hardware())" in source and "power_page_items([], actions, power_hardware())" in source,
      "page 23 filtrée à l'affichage")

# Aucun texte public qui se compare à Windows 11.
welcome = (ROOT / "testexe/src/shell/welcome.rs").read_text(encoding="latin-1")
check("Windows 11" not in welcome, "« Windows 11 » dans la visite guidée")
accounts = runpy.run_path(str(HERE / "winlux-accounts.py"), run_name="accounts_doc")
check("Windows 11" not in (accounts.get("__doc__") or ""), "« Windows 11 » dans l'aide de winlux-accounts")

# Explorateur : Utilisateurs > damien > Téléchargements.
explorer = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="latin-1")
check('fn crumb_label' in explorer and 'b"Utilisateurs", b"Users", b"Usuarios", b"Benutzer"' in explorer,
      "segment « Users » localisé")
check("segs[i].0 = crumb_label(segs[i].0, i == 2, users && i == 4);" in explorer, "dossiers du compte localisés dans le chemin")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("OK: Alimentation selon le matériel détecté, textes sans « Windows 11 », chemin localisé")
