#!/usr/bin/python3
"""Lecteur multimédia : un seul flux audio par lecture (#1013).

Le lecteur tuait et relançait ffmpeg à chaque pause et à chaque pas de
volume : le flux PipeWire disparaissait puis revenait (vu « par
intermittence » dans pactl, #1010), avec un trou d'environ 250 ms, un clic
possible et le réglage du mélangeur perdu. Vérifie dans le source (sans
compiler, pour test-all --rapide) que la pause suspend ffmpeg et que le
volume se règle en direct ; le test cargo volume_en_direct_sans_relancer
vérifie la commande contre un vrai ffmpeg.

Banc 823294d : même avec ffmpeg suspendu, le flux restait « Bouchonné : non »
et à 100 % dans pactl, le volume étant appliqué au signal. Le son passe donc
par le moteur audio du runtime (PCM sur le descripteur 3) : la pause rend la
source inactive et le moteur bouchonne le flux PipeWire (pw_stream_set_active),
le volume du lecteur devient celui du flux (pw_stream_set_control). Le test
cargo ignoré un_seul_flux_pendant_pause_et_volume le vérifie avec pactl sur un
vrai serveur.
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
media = (ROOT / "runtime/src/dlls/winlux_media.rs").read_text()
player = (ROOT / "testexe/src/shell/mediaplayer.rs").read_text()
failures = []


def check(cond, message):
    if not cond:
        failures.append(message)


play_args = media[media.index("pub fn build_play_args"):media.index("pub fn audio_env")]
check('"-nostdin"' not in play_args, "ffmpeg de lecture lancé avec -nostdin : aucune commande en direct possible")
check("volume@vol=" in play_args, "filtre de volume sans nom : la commande en direct ne le trouve pas")
check('format!("cvolume@vol -1 volume {:.2}\\n"' in media, "commande de volume en direct absente")
check("libc::SIGSTOP" in media and "libc::SIGCONT" in media, "la pause ne suspend plus ffmpeg")
check('"WinluxMediaControl" => fn winlux_media_control' in media, "WinluxMediaControl non exporté")
check("WinluxMediaControl" in (ROOT / "docs/compat/kernel32-surface.txt").read_text().split(), "surface kernel32 sans WinluxMediaControl")

audio = (ROOT / "runtime/src/audio/mod.rs").read_text()
pipewire = (ROOT / "runtime/src/audio/pipewire.rs").read_text()
check('pub const ENGINE_AUDIO: (&str, &str) = ("f32le", "pipe:3");' in media
      and "Some((ENGINE_AUDIO.0.to_string(), ENGINE_AUDIO.1.to_string()))" in media,
      "le son du lecteur ne passe plus par le moteur audio (flux recréé, ni bouchonné ni réglable)")
check("a.paused.store(pause, Ordering::SeqCst)" in media, "la pause ne bouchonne plus le flux du moteur")
check("crate::audio::set_app_volume(crate::audio::percent_to_gain(value))" in media,
      "le volume du lecteur n'est plus le volume du flux")
check("o.set_paused(all_paused)" in audio and "fn active(&self) -> bool" in audio,
      "le moteur ne bouchonne plus un flux dont toutes les sources sont en pause")
check('"pw_stream_set_active"' in pipewire and '"pw_stream_set_control"' in pipewire
      and "SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008" in pipewire,
      "PipeWire : pause ou volume du flux non transmis au serveur")

pause = player[player.index("unsafe fn play_pause"):player.index("unsafe fn seek")]
check("WinluxMediaControl(SESSION, 1, 1)" in pause and "WinluxMediaControl(SESSION, 1, 0)" in pause,
      "la pause du lecteur ferme encore le flux")
volume = player[player.index("unsafe fn apply_volume"):player.index("unsafe fn restart_soon")]
check("WinluxMediaControl(SESSION, 2, volume)" in volume, "le volume du lecteur relance encore la lecture")
change = player[player.index("unsafe fn change_volume"):]
change = change[:change.index("\n}\n")]
check("apply_volume()" in change and "restart_soon()" not in change, "flèches ↑/↓ : relance au lieu du réglage en direct")
mute = re.findall(r"MUTED = !MUTED;\s*\n\s*(\w+)\(\);", player)
check(mute and all(call == "apply_volume" for call in mute), f"muet : relance au lieu du réglage en direct ({mute})")

if failures:
    raise SystemExit("ÉCHEC test-media-one-stream :\n- " + "\n- ".join(failures))
print("OK test-media-one-stream : un seul flux, bouchonné en pause, volume du lecteur sur le flux")
