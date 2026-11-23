# #2908 : quand la définition ou les écrans changent, winlux-snap watch lance
# « winlux-snap resnap », qui replace chaque fenêtre ancrée dans sa case
# recalculée ; vérifié sous openbox réel par test-snap-mouse.py.
_s_2908 = (SCRIPTS / "winlux-snap.py").read_text(encoding="utf-8")
ok("def resnap" in _s_2908 and "def screen_changed" in _s_2908
   and "resnap : moitié droite" in (SCRIPTS / "test-snap-mouse.py").read_text(encoding="utf-8"),
   "Ancrage : les fenêtres ancrées suivent un changement de définition (#2908)")
