#!/usr/bin/python3
"""Son des FFmpeg auxiliaires nommé dans le mélangeur (#1010).

ISO 7720913 : pendant une vidéo du lecteur multimédia, `pactl list
sink-inputs` montrait « Lavf62.3.100 », binaire « ffmpeg » : le flux venait
du ffmpeg de winlux_media, sous le nom générique de libavformat. Le
mélangeur par application ne pouvait pas présenter le lecteur et la
restauration du volume par application.name ne le retrouvait pas. Les
enregistrements (winlux_capture) avaient le même angle mort côté entrées.

Vérifie dans le source (sans compiler, pour test-all --rapide) ; les tests
cargo (nom_du_flux_dans_le_melangeur, ligne_de_commande_lecture,
arguments_ffmpeg_*) vérifient le comportement.
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
backend = (ROOT / "runtime/src/audio/backend.rs").read_text()
media = (ROOT / "runtime/src/dlls/winlux_media.rs").read_text()
capture = (ROOT / "runtime/src/dlls/winlux_capture.rs").read_text()
pipewire = (ROOT / "runtime/src/audio/pipewire.rs").read_text()
failures = []


def check(cond, message):
    if not cond:
        failures.append(message)


player = re.search(r'"mediaplayer\.exe",\s*\[([^\]]*)\]', backend)
names = re.findall(r'"([^"]+)"', player[1]) if player else []
check(names == ["Lecteur multimédia", "Media Player", "Reproductor multimedia", "Medienwiedergabe"],
      f"lecteur multimédia non traduit dans les 4 langues : {names}")
check('starts_with("/opt/winlux/apps/")' in backend, "un .exe invité nommé mediaplayer.exe prendrait le nom du lecteur Winlux")
check("pub fn process_binary()" in backend and "application.process.binary=\\\"{binary}\\\"" in pipewire,
      "le flux PipeWire du runtime ne donne plus le vrai binaire")

check(re.search(r'p\.audio_out\.0 == "pulse".*?"-name"\.into\(\),\s*p\.audio_app\.clone\(\),\s*"-stream_name"', media, re.S),
      "winlux_media ne nomme plus le flux pulse de ffmpeg (-name/-stream_name)")
check("audio_app: crate::audio::backend::app_name()" in media, "le plan de lecture n'emporte pas le nom de l'application")
check(re.search(r"cmd\.envs\(audio_env\(", media), "le ffmpeg du lecteur ne reçoit pas PULSE_PROP/PIPEWIRE_PROPS")
env = media[media.index("pub fn audio_env"):media.index("fn audio_output")]
check('"PULSE_PROP"' in env and "application.process.binary=" in env and '"PIPEWIRE_PROPS"' in env,
      "audio_env ne règle pas application.process.binary pour libpulse et PipeWire")

check(re.search(r'format == "pulse".*?"-name"\.to_string\(\),\s*app\.to_string\(\)', capture, re.S),
      "les enregistrements (micro, son du système) restent « Lavf… »")
check(re.search(r"winlux_media::audio_env\(\s*&app", capture), "le ffmpeg d'enregistrement ne reçoit pas le vrai binaire")

# Contrôle réel en VM (release.yml) : ffmpeg joue un son avec les mêmes options
# et `pactl list sink-inputs` ne doit montrer ni « Lavf… » ni « ffmpeg ».
guest = (ROOT / "scripts/test-native-session-guest.py").read_text()
check("def stream_names()" in guest and "stream_names()" in guest[guest.index("def system_services"):]
      and '"Lavf" not in listing' in guest and 'application.process.binary = \"mediaplayer.exe\"' in guest,
      "le test en VM ne vérifie plus le nom du flux pendant une lecture")

if failures:
    raise SystemExit("ÉCHEC test-audio-stream-names :\n- " + "\n- ".join(failures))
print("OK test-audio-stream-names : flux ffmpeg nommés (lecteur multimédia, enregistrements)")
