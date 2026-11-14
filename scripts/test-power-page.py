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
# #2150 : « Veille prolongée » seulement quand le noyau a une zone de reprise.
ready = settings["hibernate_ready"]
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    check(ready(root) is False, "veille prolongée prête sans /sys/power")
    (root / "sys/power").mkdir(parents=True)
    (root / "sys/power/state").write_text("freeze mem disk\n")
    (root / "sys/power/resume").write_text("0:0\n")
    check(ready(root) is False, "veille prolongée prête sans zone de reprise (0:0)")
    (root / "sys/power/resume").write_text("259:2\n")
    check(ready(root) is True, "zone de reprise configurée : veille prolongée prête")
    (root / "sys/power/state").write_text("freeze mem\n")
    check(ready(root) is False, "noyau sans « disk » : veille prolongée prête")
_, no_hibernate = items([], actions, (True, True, True), False)
check("power-hibernate" not in [key for _, key in no_hibernate] and "power-suspend" in [key for _, key in no_hibernate],
      "bouton « Veille prolongée » affiché alors qu'elle n'est pas prête")
_source = (HERE / "winlux-settings.py").read_text()
check("hibernate_ready()" in _source
      and 'if ready or key != "hibernate"' in _source, "page 23 : veille prolongée filtrée (bouton, capot, bouton d'alimentation)")
check(settings["_TR"]["Alimentation"] == ("Power", "Energía", "Energie"), "« Alimentation » en quatre langues")
source = (HERE / "winlux-settings.py").read_text()
check('if not power_hardware()[0]: PAGES[23] = _t("Alimentation")' in source, "titre sans batterie")
check(source.count("power_page_items(") >= 3 and "idle_sleep=idle_sleep_available(" in source and "hibernate_ready()" in source,
      "page 23 filtrée à l'affichage")

# #2151 : en VM sans batterie, le démon ne met jamais en veille sur inactivité ;
# les délais « Veille » sont masqués et une phrase dit pourquoi.
idle = settings["idle_sleep_available"]
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    check(idle(False, root) is True, "sans /proc/cpuinfo : délais de veille gardés")
    (root / "proc").mkdir()
    (root / "proc/cpuinfo").write_text("processor\t: 0\nflags\t\t: fpu vme sse2\n")
    check(idle(False, root) is True, "vrai PC sans batterie : délais de veille gardés")
    (root / "proc/cpuinfo").write_text("processor\t: 0\nflags\t\t: fpu vme sse2 hypervisor\n")
    check(idle(False, root) is False, "VM sans batterie : pas de veille sur inactivité")
    check(idle(True, root) is True, "VM avec batterie : délais de veille gardés")
vm_fields, vm_actions = items(fields, actions, (False, False, False), idle_sleep=False)
vm_keys = [key for _, key in vm_fields] + [key for _, key in vm_actions]
check(not any(key.startswith("power-sleep-") for key in vm_keys), f"VM sans batterie : délais de veille affichés {vm_keys}")
check("power-suspend" in vm_keys and "power-screen-ac" in vm_keys, "VM sans batterie : « Mettre en veille » et écran gardés")
note = "Machine virtuelle sans batterie : elle ne se met pas en veille toute seule, pour ne pas se figer. « Mettre en veille » reste possible."
check(note in source and len(settings["_TR"].get(note, ())) == 3, "phrase de la VM sans batterie en quatre langues")

# Menus déroulants plutôt que des boutons qui font défiler la valeur (#895).
delay_text = settings["delay_text"]
battery = settings["battery_text"]
check(battery("99% Not charging") == "99 % · chargée" and battery("54% Discharging") == "54 % · sur batterie"
      and battery("80% Charging") == "80 % · en charge" and battery("40% Not charging") == "40 % · branchée, pas en charge"
      and battery("100% Full") == "100 % · chargée" and battery("70% Unknown") == "70 %", "état de la batterie lisible (#902)")
check(all(len(settings["_TR"].get(word, ())) == 3 for word in settings["BATTERY_STATES"].values()), "états de batterie en 4 langues")
check(delay_text(300) == "5 min" and delay_text(3600) == "1 h" and delay_text(0) == "Jamais", "délais lisibles")
check(all(a.startswith(settings["POWER_CHOICES"]) for _, a in actions if a.startswith(("power-screen", "power-sleep", "power-lid", "power-button"))),
      "délais, capot et bouton en menus")
check("self.power_controls(choices)" in source and 'f"power-set-' in source, "page 23 en menus déroulants")
import re as _re
root_policy = runpy.run_path(str(HERE / "winlux-root.py"), run_name="power_root")["CONTROL_POLICY"]
def allowed(action):
    return any(_re.fullmatch(pattern, action) for pattern, _ in root_policy)
check(allowed("power-set-screen-ac-300") and allowed("power-set-lid-sleep") and not allowed("power-set-screen-ac-3;x"),
      "relais root : valeurs choisies acceptées, le reste refusé")
control = (HERE / "winlux-control.sh").read_text()
check('winlux --power set "${setting%-*}" "$value"' in control and 'winlux --power set lid "${action#power-set-lid-}"' in control,
      "winlux-control applique la valeur choisie")
check('"display-"' in source and "Adaptation à l'écran" in source, "page Écran : définition en menu")

# Aucun texte public qui se compare à Windows 11.
welcome = (ROOT / "testexe/src/shell/welcome.rs").read_text(encoding="latin-1")
check("Windows 11" not in welcome, "« Windows 11 » dans la visite guidée")
accounts = runpy.run_path(str(HERE / "winlux-accounts.py"), run_name="accounts_doc")
check("Windows 11" not in (accounts.get("__doc__") or ""), "« Windows 11 » dans l'aide de winlux-accounts")
# Aides affichées (docstring imprimée par les outils winlux-*, #903).
import ast
for tool in sorted(HERE.glob("winlux-*.py")):
    text = tool.read_text()
    if "__doc__" not in text: continue
    doc = ast.get_docstring(ast.parse(text)) or ""
    check("Windows 11" not in doc, f"« Windows 11 » dans l'aide de {tool.stem}")

# Explorateur : Utilisateurs > damien > Téléchargements.
explorer = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="latin-1")
check('fn crumb_label' in explorer and 'b"Utilisateurs", b"Users", b"Usuarios", b"Benutzer"' in explorer,
      "segment « Users » localisé")
check("segs[i].0 = crumb_label(segs[i].0, i == 2, users && i == 4);" in explorer, "dossiers du compte localisés dans le chemin")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("OK: Alimentation selon le matériel détecté, textes sans « Windows 11 », chemin localisé")
