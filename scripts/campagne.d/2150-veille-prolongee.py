# Paramètres > Alimentation : « Veille prolongée » (bouton, capot, bouton
# d'alimentation) seulement quand le noyau a une zone de reprise (#2150).
_s = (ROOT / "scripts/winlux-settings.py").read_text(encoding="utf-8")
ok("def hibernate_ready" in _s and 'resume not in ("", "0:0")' in _s, "veille prolongée prête seulement avec une zone de reprise (#2150)")
ok("power_page_items([], actions, power_hardware(), hibernate_ready())" in _s and 'if ready or key != "hibernate"' in _s,
   "bouton et choix « veille prolongée » masqués quand elle n'est pas prête (#2150)")
