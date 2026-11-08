#!/usr/bin/python3
"""Dictée vocale et sous-titres en direct (#662) : découpage aux silences,
nettoyage du texte, saisie, détection du micro, modèle vérifié par empreinte,
moteur factice (et vrai moteur s'il est là), branchements.

Sous un écran (DISPLAY, xvfb-run en CI), la barre de dictée et la bande de
sous-titres tournent aussi sur un fichier son enregistré (WINLUX_SPEECH_INPUT) :
le micro n'existe pas en machine virtuelle.
"""
import array
import hashlib
import json
import math
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import wave
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SCRIPT = HERE / "winlux-speech.py"
m = runpy.run_path(str(SCRIPT), run_name="speech_test")
RATE = m["RATE"]


def tone(seconds, amplitude=6000, freq=220):
    samples = array.array("h", (int(amplitude * math.sin(2 * math.pi * freq * i / RATE)) for i in range(int(RATE * seconds))))
    if sys.byteorder == "big": samples.byteswap()
    return samples.tobytes()


def silence(seconds, amplitude=30):
    return tone(seconds, amplitude, 50)


def segments(pcm, **options):
    cutter = m["Segmenter"](**options)
    out = []
    frame = m["FRAME_BYTES"]
    for at in range(0, len(pcm), frame):
        part = cutter.feed(pcm[at:at + frame].ljust(frame, b"\0"))
        if part: out.append(part)
    rest = cutter.flush()
    if rest: out.append(rest)
    return out


# Traductions : les quatre langues de l'interface.
for key, row in m["_TR"].items():
    assert len(row) == 3 and all(row), key
pages = (HERE / "winlux-settings-pages.py").read_text()
for text in ("Voix", "Dictée vocale", "Sous-titres en direct", "Langue de la dictée", "Son du PC et micro",
             "Windows + Ctrl + L : une bande affiche le texte de ce qui sort des haut-parleurs."):
    assert f"    '{text}': (" in pages, text

# Découpage : deux phrases séparées par un silence, une phrase trop longue coupée.
speech = silence(1) + tone(1) + silence(1) + tone(1.5) + silence(1.2)
parts = segments(speech, silence_ms=700, max_ms=12000)
assert len(parts) == 2, [len(p) for p in parts]
assert all(len(p) >= RATE * 2 for p in parts)
assert len(segments(silence(1) + tone(9) + silence(1), max_ms=4000)) == 3
assert segments(silence(3)) == []
# Un claquement bref (100 ms) n'est pas une phrase.
assert segments(silence(1) + tone(0.1) + silence(1.5)) == []

# Rééchantillonnage sans ffmpeg : 22 050 Hz stéréo → 16 kHz mono, durée gardée.
stereo = array.array("h", [1000, 3000] * 22050)
mono = m["pcm16_to_16k_mono"](stereo.tobytes(), 22050, 2)
assert len(mono) == 2 * RATE and set(array.array("h", mono)) == {2000}
assert m["pcm16_to_16k_mono"](b"ab", RATE, 1) == b"ab"

# Nettoyage du texte et textes inventés sur du silence.
clean = m["clean_text"]
assert clean(" Bonjour , tout le monde .\n Ça va ? ") == "Bonjour, tout le monde. Ça va ?"
assert clean("[Musique] (applaudissements) Merci beaucoup ♪") == "Merci beaucoup"
assert clean("Sous-titres réalisés par la communauté d'Amara.org") == ""
assert clean("Thank you for watching!") == "" and clean("[BLANK_AUDIO]") == ""

# Saisie : espace entre deux phrases, « à la ligne » tape Entrée.
commands, ended = m["typing_commands"]("Bonjour.", None)
assert commands == [["xdotool", "type", "--clearmodifiers", "--delay", "6", "--", "Bonjour."]] and ended is True
commands, ended = m["typing_commands"]("Deuxième phrase.", True)
assert commands[0][-1] == " Deuxième phrase."
commands, ended = m["typing_commands"]("À la ligne.", True)
assert commands == [["xdotool", "key", "--clearmodifiers", "Return"]] and ended is False
assert m["typing_commands"]("Suite.", False)[0][0][-1] == "Suite."

# Micro : ni moniteur de sortie ni source factice.
sources = [{"name": "alsa_output.pci.analog-stereo.monitor", "monitor_of_sink": "alsa_output.pci.analog-stereo"},
           {"name": "auto_null.monitor", "monitor_of_sink": "auto_null"},
           {"name": "alsa_input.pci.analog-stereo", "monitor_of_sink": "n/a"}]
assert m["microphones"](sources) == ["alsa_input.pci.analog-stereo"]
assert m["microphones"](sources[:2]) == []

# Réglages : valeurs inconnues refusées.
with tempfile.TemporaryDirectory() as folder:
    env = dict(os.environ, XDG_CONFIG_HOME=folder, WINLUX_LANG="fr")
    run = lambda *args, **kw: subprocess.run([sys.executable, "-I", str(SCRIPT), *args], env=kw.get("env", env),
                                             capture_output=True, text=True, timeout=60)
    assert run("set", "captions_position", "top").returncode == 0
    assert run("set", "captions_position", "milieu").returncode == 2
    assert run("set", "dictation_language", "en").returncode == 0
    assert "captions_position=top" in (Path(folder) / "winlux/speech.conf").read_text()

# Modèle : empreinte et taille vérifiées, fichier refusé sinon.
with tempfile.TemporaryDirectory() as folder:
    folder = Path(folder)
    good = folder / "bon.bin"
    good.write_bytes(b"modele-de-test" * 1000)
    saved = dict(m["MODELS"]["base"])
    os.environ["WINLUX_SPEECH_MODEL_DIR"] = str(folder / "modeles")
    try:
        m["MODELS"]["base"].update(bytes=good.stat().st_size, sha256=hashlib.sha256(good.read_bytes()).hexdigest(), file="bon.bin")
        os.environ["WINLUX_SPEECH_MODEL_URL"] = "file://" + str(folder) + "/{file}"
        assert m["download"]("base") is True and (folder / "modeles/bon.bin").is_file()
        (folder / "modeles/bon.bin").unlink()
        m["MODELS"]["base"]["sha256"] = "0" * 64
        assert m["download"]("base") is False
        assert not (folder / "modeles/bon.bin").exists() and not (folder / "modeles/bon.part").exists()
    finally:
        m["MODELS"]["base"].clear()
        m["MODELS"]["base"].update(saved)
        os.environ.pop("WINLUX_SPEECH_MODEL_URL", None)
        os.environ.pop("WINLUX_SPEECH_MODEL_DIR", None)
# Empreintes livrées : SHA-256 complets, révision figée, tailles annoncées cohérentes.
for name, info in m["MODELS"].items():
    assert len(info["sha256"]) == 64 and int(info["sha256"], 16) >= 0, name
    assert abs(info["bytes"] / 1e6 - info["mb"]) < info["mb"] * 0.1, name
assert len(m["MODEL_REVISION"]) == 40 and "{revision}" in m["MODEL_URL"]

# Moteur factice : arguments, langue, découpage d'un enregistrement, statut.
with tempfile.TemporaryDirectory() as folder:
    folder = Path(folder)
    models = folder / "modeles"
    models.mkdir()
    with open(models / m["MODELS"]["base"]["file"], "wb") as out:
        out.truncate(m["MODELS"]["base"]["bytes"])
    fake = folder / "whisper-cli"
    fake.write_text("#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/args\"\nprintf ' Bonjour , ceci est un essai .\\n'\n")
    fake.chmod(0o755)
    pactl = folder / "pactl"
    pactl.write_text("#!/bin/sh\ncase \"$*\" in\n  get-default-sink) echo alsa_output.test ;;\n"
                     "  *sources*) echo '" + json.dumps(sources) + "' ;;\nesac\n")
    pactl.chmod(0o755)
    (folder / "parec").write_text("#!/bin/sh\nexit 0\n")
    (folder / "parec").chmod(0o755)
    recording = folder / "enregistrement.wav"
    with wave.open(str(recording), "wb") as out:
        out.setnchannels(1); out.setsampwidth(2); out.setframerate(RATE)
        out.writeframes(speech)
    env = dict(os.environ, WINLUX_SPEECH_ENGINE=str(fake), WINLUX_SPEECH_MODEL_DIR=str(models),
               XDG_CONFIG_HOME=str(folder / "config"), XDG_RUNTIME_DIR=str(folder), WINLUX_LANG="fr",
               PATH=f"{folder}:{os.environ.get('PATH', '/usr/bin')}")
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "transcribe", str(recording), "fr"], env=env,
                         capture_output=True, text=True, timeout=60)
    assert out.returncode == 0 and out.stdout == "Bonjour, ceci est un essai. Bonjour, ceci est un essai.\n", out
    calls = (folder / "args").read_text().splitlines()
    assert len(calls) == 2 and all("-l fr -nt -np" in call for call in calls), calls
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "status"], env=env, capture_output=True, text=True, timeout=60)
    state = dict(line.split("=", 1) for line in out.stdout.split())
    assert state["engine"] == "1" and state["model"] == "base" and state["mic"] == "1" and state["output"] == "1", state
    assert state["captions"] == "off" and state["size_base"] == "57" and state["size_small"] == "181", state
    # Sans micro : la dictée n'a pas lieu d'être (Paramètres la masquent).
    pactl.write_text("#!/bin/sh\ncase \"$*\" in\n  get-default-sink) echo alsa_output.test ;;\n"
                     "  *sources*) echo '" + json.dumps(sources[:2]) + "' ;;\nesac\n")
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "status"], env=env, capture_output=True, text=True, timeout=60)
    assert "mic=0" in out.stdout and "output=1" in out.stdout, out.stdout
    # Sans moteur : statut à 0, transcription refusée.
    bare = dict(env, WINLUX_SPEECH_ENGINE="", PATH=str(folder / "vide"))
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "status"], env=bare, capture_output=True, text=True, timeout=60)
    assert "engine=0" in out.stdout and "mic=0" in out.stdout and "output=0" in out.stdout, out.stdout

    # Barre de dictée et bande de sous-titres sur l'enregistrement (écran requis).
    if os.environ.get("DISPLAY") and subprocess.run([sys.executable, "-c", "import gi; gi.require_version('Gtk', '3.0'); "
                                                     "from gi.repository import Gtk"], capture_output=True).returncode == 0:
        for command in ("dictate", "captions"):
            typed = folder / f"{command}.txt"
            run_env = dict(env, WINLUX_SPEECH_INPUT=str(recording), WINLUX_SPEECH_TYPED=str(typed))
            out = subprocess.run([sys.executable, str(SCRIPT), command], env=run_env, capture_output=True, text=True, timeout=120)
            assert out.returncode == 0, (command, out.stderr[-2000:])
            # #1003 : aucun avertissement de dépréciation (GLib.unix_signal_add…).
            assert "Deprecat" not in out.stderr, (command, out.stderr[-2000:])
            lines = typed.read_text().splitlines()
            assert lines and lines[0] == "Bonjour, ceci est un essai.", (command, lines)
            if command == "dictate": assert lines[1:] == [" Bonjour, ceci est un essai."], lines
            assert not (folder / f"winlux/speech-{'dictation' if command == 'dictate' else 'captions'}.pid").exists()
        print("barre de dictée et sous-titres : OK")

# Vrai moteur et vrai modèle s'ils sont là (poste de test, runner) : phrase anglaise reconnue.
sample = os.environ.get("WINLUX_SPEECH_SAMPLE")
if sample and m["engine"]() and m["current_model"]():
    text = m["transcribe_file"](sample, "en")
    word = os.environ.get("WINLUX_SPEECH_SAMPLE_WORD", "country")
    assert text and word in text.lower(), text
    print("vrai moteur :", text)

# Branchements : raccourcis, migration des comptes existants, image, Paramètres.
ns = "{http://openbox.org/3.4/rc}"
rc = ET.parse(ROOT / "integration/native/config/openbox/rc.xml").getroot()
bindings = {k.get("key"): k.findtext(f"{ns}action/{ns}command") for k in rc.iter(f"{ns}keybind")}
assert bindings.get("W-h") == "winlux-speech dictate", bindings
assert bindings.get("W-C-l") == "winlux-speech captions", bindings
config = (HERE / "winlux-native-config.py").read_text()
assert '"W-h", "W-C-l"' in config
assert "'winlux-speech.py:winlux-speech'" in (HERE / "stage-desktop-components.sh").read_text()
assert "def sections_speech" in pages and "self.sections_speech()" in pages
body = pages[pages.index("def sections_speech"):pages.index("def set_contrast")]
assert "if not mic and not output: return" in body and "if mic:" in body and "if output:" in body
assert body.index("if mic:") < body.index("_t(\"Dictée vocale\")")
assert '"speech": 12' in (HERE / "winlux-settings.py").read_text()
# Rien n'est envoyé en ligne : le seul réseau est le téléchargement du modèle.
source = SCRIPT.read_text()
assert "GLibUnix.signal_add" in source and source.count("GLib.unix_signal_add") == 1
assert source.count("urlopen(") == 1 and "huggingface.co/ggerganov/whisper.cpp/resolve/{revision}/" in source
print("test-speech : OK")
