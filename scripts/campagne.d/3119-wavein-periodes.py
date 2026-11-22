# waveIn lisait ALSA par périodes de ~125 ms : les petits tampons des
# programmes (PortAudio MME) ne pouvaient tout recevoir, plus de la moitié des
# échantillons était perdue (#3119). Le test unitaire nommé ici vérifie la
# taille de période demandée.
_wm = (ROOT / "runtime" / "src" / "dlls" / "winmm.rs").read_text(encoding="utf-8")
ok("fn capture_waveIn_en_periodes_courtes_3119" in _wm,
   "waveIn capture par périodes d'au plus 10 ms (#3119)")
