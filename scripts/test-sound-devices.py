#!/usr/bin/python3
"""Paramètres > Son sur un vrai PC : sortie factice, cartes son et firmwares.

Test VM/PC du 3 novembre 2026 : sur le portable de damien, les haut-parleurs et
le micro intégrés n'apparaissaient pas ; seule la « Sortie factice » de PipeWire
(auto_null) était proposée. Vérifie :
- la sortie/entrée factice n'est jamais proposée comme haut-parleur ou micro ;
- sans aucune carte, la page le dit clairement et renvoie vers Matériel et
  pilotes, puis se met à jour seule quand une carte apparaît ;
- l'image porte les firmwares là où le noyau les cherche (/lib/firmware), le
  firmware SOF, les profils UCM et tous les pilotes son.
"""
import json
import os
from pathlib import Path
import runpy
import stat
import tempfile

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-son-"))
state = tmp / "pactl-state.json"
fake = tmp / "bin/pactl"
fake.parent.mkdir()
fake.write_text(f"""#!/usr/bin/python3
import json, sys
data = json.load(open({str(state)!r}))
args = sys.argv[1:]
if args == ["info"]:
    print("Server Name: PulseAudio (on PipeWire 1.4.2)"); sys.exit(0)
if args[:3] == ["-f", "json", "list"]:
    print(json.dumps(data.get(args[3], []))); sys.exit(0)
if args and args[0].startswith("get-default-"):
    print(data.get(args[0], "")); sys.exit(0)
sys.exit(0)
""")
fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
os.environ["PATH"] = f"{fake.parent}:{os.environ['PATH']}"

NULL_SINK = {"index": 30, "name": "auto_null", "description": "Sortie factice", "mute": False,
             "volume": {"front-left": {"value_percent": "40%"}},
             "properties": {"factory.name": "support.null-audio-sink", "node.name": "auto_null"}}
NULL_SOURCE = {"index": 31, "name": "auto_null.monitor", "description": "Monitor of Sortie factice",
               "monitor_of_sink": "auto_null", "properties": {}}
SPEAKERS = {"index": 50, "name": "alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Speaker__sink",
            "description": "Haut-parleurs", "mute": False, "volume": {"front-left": {"value_percent": "62%"}},
            "properties": {"device.api": "alsa", "factory.name": "api.alsa.pcm.sink"}}
MIC = {"index": 51, "name": "alsa_input.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Mic1__source",
       "description": "Micro numérique", "mute": False, "volume": {"front-left": {"value_percent": "80%"}},
       "properties": {"device.api": "alsa"}}


def write(sinks, sources, default_sink="", default_source=""):
    state.write_text(json.dumps({"sinks": sinks, "sources": sources,
                                 "get-default-sink": default_sink, "get-default-source": default_source}))


failures = []


def check(cond, message):
    if not cond:
        failures.append(message)


os.environ["WINLUX_LANG"] = "fr"
pages = runpy.run_path(str(HERE / "winlux-settings-pages.py"), run_name="sound_test")
audio_devices, page_available = pages["audio_devices"], pages["page_available"]
PAGE_SOUND, PAGE_HARDWARE = pages["PAGE_SOUND"], pages["PAGE_HARDWARE"]

# 1. Aucune carte reliée : PipeWire ne propose que sa sortie factice.
write([NULL_SINK], [NULL_SOURCE], "auto_null", "auto_null.monitor")
sinks, sources = audio_devices()
check(sinks == [] and sources == [], f"sortie factice proposée : {sinks} {sources}")
check(page_available(PAGE_SOUND, {}), "page Son masquée alors que le serveur son tourne")


class Page(pages["ExtraPages"]):
    """Assez de la fenêtre Paramètres pour construire la page Son."""
    def __init__(self):
        self.calls, self.page, self.opened = [], PAGE_SOUND, []

    def section(self, title, *_): self.calls.append(("section", title))
    def card(self): return object()
    def hint(self, text, parent=None): self.calls.append(("hint", text))
    def actions(self, parent, buttons): self.calls.extend(("action", label, cb) for label, cb in buttons)
    def combo(self, parent, title, items, default, changed): self.calls.append(("combo", title, items, default))
    def volume_row(self, parent, title, value, changed): self.calls.append(("volume", title, value))
    def switch(self, parent, title, value, changed): self.calls.append(("switch", title))
    def open_page(self, page): self.opened.append(page)
    def extra_refresh(self): self.calls.append(("refresh",))


timers = []
pages["GLib"].timeout_add_seconds = lambda seconds, func: timers.append(func) or 99
page = Page()
page.page_sound()
combos = [c for c in page.calls if c[0] == "combo"]
hints = " ".join(c[1] for c in page.calls if c[0] == "hint")
check(not combos, f"liste de haut-parleurs proposée sans carte : {combos}")
check("Aucun haut-parleur" in hints and "Matériel et pilotes" in hints, f"message absent : {hints!r}")
check("factice" not in repr(page.calls).lower(), "« Sortie factice » affichée")
actions = [c for c in page.calls if c[0] == "action"]
check(actions and actions[0][1] == "Matériel et pilotes", f"bouton Matériel et pilotes absent : {actions}")
if actions:
    actions[0][2]()
    check(page.opened == [PAGE_HARDWARE], f"le bouton ouvre {page.opened}")
# Carte reconnue plus tard (firmware installé, casque USB) : la page se met à jour.
check(len(timers) == 1, "pas de surveillance des cartes son")
if timers:
    check(timers[0]() is True, "surveillance arrêtée trop tôt")
    write([NULL_SINK, SPEAKERS], [NULL_SOURCE, MIC], SPEAKERS["name"], MIC["name"])
    check(timers[0]() is False and ("refresh",) in page.calls, "la page ne se met pas à jour quand la carte apparaît")

# 2. Carte reconnue : haut-parleurs et micro, sans la sortie factice.
write([NULL_SINK, SPEAKERS], [NULL_SOURCE, MIC], SPEAKERS["name"], MIC["name"])
page = Page()
page.page_sound()
combos = {c[1]: c[2] for c in page.calls if c[0] == "combo"}
check(combos.get("Haut-parleurs ou casque") == [(SPEAKERS["name"], "Haut-parleurs")], f"sortie : {combos}")
check(combos.get("Micro") == [(MIC["name"], "Micro numérique")], f"entrée : {combos}")
check(("volume", "Volume", 62) in page.calls, "volume des haut-parleurs")
check(not [c for c in page.calls if c[0] == "hint" and "Aucun haut-parleur" in c[1]], "message « aucun » avec une carte")

# 3. Traductions du message.
tr = pages["_TR"]
for text in ("Aucun haut-parleur, casque ni micro n’est reconnu sur ce PC. Si l’ordinateur en a, son pilote ou son "
             "firmware audio manque : la page Matériel et pilotes l’installe.", "Matériel et pilotes"):
    check(text in tr and len(tr[text]) == 3 and all(tr[text]), f"traduction manquante : {text[:40]}")

# 4. Image : firmwares à l'endroit lu par le noyau, SOF, UCM, pilotes son.
stage_script = (HERE / "stage-desktop-components.sh").read_text()
check('ln -sfn ../usr/lib/firmware "$stage/lib/firmware"' in stage_script, "lien /lib/firmware → /usr/lib/firmware absent")
packages = (HERE / "prepare-desktop-packages.py").read_text()
check('"firmware-sof-signed"' in packages and '"alsa-ucm-conf"' in packages and '"linux-firmware"' in packages,
      "firmware SOF, profils UCM ou linux-firmware absents des paquets")
build = (HERE / "build-live-iso.sh").read_text()
check("for directory in sound drivers/soundwire" in build, "pilotes son ASoC/SoundWire absents de l'image")

# Le lien lui-même : on rejoue le bloc du script sur un faux stage.
import subprocess
fake_stage = tmp / "stage"
(fake_stage / "usr/lib/firmware/intel/sof").mkdir(parents=True)
(fake_stage / "usr/lib/firmware/intel/sof/sof-tgl.ri").write_text("x")
(fake_stage / "lib/firmware/intel-ucode").mkdir(parents=True)
(fake_stage / "lib/firmware/intel-ucode/06-8c-01").write_text("µcode")
block = stage_script[stage_script.index('if test -d "$stage/usr/lib/firmware"'):]
block = block[:block.index("\nfi\n") + 4]
subprocess.run(["sh", "-eu", "-c", block], env={**os.environ, "stage": str(fake_stage)}, check=True)
firmware = fake_stage / "lib/firmware"
check(firmware.is_symlink() and (firmware / "intel/sof/sof-tgl.ri").is_file(), "firmware SOF introuvable par /lib/firmware")
check((firmware / "intel-ucode/06-8c-01").is_file(), "firmware déjà présent dans /lib/firmware perdu")

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    raise SystemExit(1)
print("OK : Son sur vrai PC (sortie factice masquée, message clair, firmwares et pilotes dans l'image)")
