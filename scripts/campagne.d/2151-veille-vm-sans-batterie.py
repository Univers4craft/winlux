# VM sans batterie : le démon ne met jamais en veille sur inactivité ; la page
# Alimentation masque les délais « Veille » et dit pourquoi (#2151).
_s = (ROOT / "scripts/winlux-settings.py").read_text(encoding="utf-8")
ok("def idle_sleep_available(" in _s and 'key.startswith("power-sleep-")' in _s,
   "délais de veille masqués en VM sans batterie (#2151)")
ok("Machine virtuelle sans batterie : elle ne se met pas en veille toute seule" in _s,
   "phrase expliquant l'absence de veille automatique en VM (#2151)")
