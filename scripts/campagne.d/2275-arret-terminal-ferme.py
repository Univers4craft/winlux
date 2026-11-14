# « winlux-power now/ask » lancé depuis un terminal survit à la fermeture de ce terminal (#2275).
_p = (ROOT / "scripts/winlux-power.py").read_text(encoding="utf-8")
_g = _p[_p.index("def gui(action, confirm, screen_only=False):"):]
_g = _g[:_g.index("\ndef ")]
ok("detach_from_terminal()" in _g and _g.index("detach_from_terminal()") < _g.index("os.fork()"), "écran d'arrêt : détaché du terminal avant de fermer les fenêtres (#2275)")
ok("signal.signal(signal.SIGHUP, signal.SIG_IGN)" in _p[_p.index("def detach_from_terminal():"):], "écran d'arrêt et gardien : SIGHUP ignoré (#2275)")
ok((ROOT / "scripts/test-all.d/2275-arret-terminal-ferme.sh").exists(), "test hors ligne du terminal fermé (#2275)")
