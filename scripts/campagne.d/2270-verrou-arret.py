# Arrêt depuis une session verrouillée : l'affichage part avant le reste, le bureau n'est jamais exposé (#2270).
_i = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
_s = _i[_i.index("safe_power() {"):]
_s = _s[:_s.index("\n}\n")]
ok("pidof Xorg" in _s and "kill -TERM $xorg_pids" in _s, "init : safe_power arrête Xorg en premier (#2270)")
ok(_s.index("kill -TERM $xorg_pids") < _s.index("/bin/busybox kill -TERM -1"), "init : Xorg arrêté avant le kill -TERM -1 qui tue l'écran de verrouillage (#2270)")
ok("kill -KILL $xorg_pids" in _s and "-lt 30" in _s, "init : Xorg tué au bout de 3 s s'il ne s'arrête pas (#2270)")
