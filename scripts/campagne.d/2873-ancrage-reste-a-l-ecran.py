# #2873 : une fenêtre dont la taille minimale dépasse sa case reste entière à
# l'écran après un ancrage (test réel sous openbox dans test-snap-mouse.py).
_s_2873 = (SCRIPTS / "winlux-snap.py").read_text(encoding="utf-8")
ok("def inside_position" in _s_2873 and "def keep_on_screen" in _s_2873
   and "ancrage-large" in (SCRIPTS / "test-snap-mouse.py").read_text(encoding="utf-8"),
   "Ancrage : une fenêtre plus large que sa case garde ses boutons à l'écran (#2873)")
