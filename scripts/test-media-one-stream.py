#!/usr/bin/python3
"""Lecteur multimédia : un seul flux audio par lecture (#1013).

Le lecteur tuait et relançait ffmpeg à chaque pause et à chaque pas de
volume : le flux PipeWire disparaissait puis revenait (vu « par
intermittence » dans pactl, #1010), avec un trou d'environ 250 ms, un clic
possible et le réglage du mélangeur perdu. Vérifie dans le source (sans
compiler, pour test-all --rapide) que la pause suspend ffmpeg et que le
volume se règle en direct ; le test cargo volume_en_direct_sans_relancer
vérifie la commande contre un vrai ffmpeg.
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
print("OK test-media-one-stream : pause et volume sans fermer le flux audio")
