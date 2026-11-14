# Paramètres > Alimentation : « Veille prolongée » (bouton, capot, bouton
# d'alimentation) seulement quand le noyau a une zone de reprise (#2150).
# Le comportement est vérifié sous Xvfb par scripts/test-power-page.py ;
# ce contrôle ne vérifie que des noms (règle des contrôles, #2349).
_s = (ROOT / "scripts/winlux-settings.py").read_text(encoding="utf-8")
_t = (ROOT / "scripts/test-power-page.py").read_text(encoding="utf-8")
ok("def hibernate_ready" in _s and "def power_page_items" in _s,
   "veille prolongée : hibernate_ready() existe dans Paramètres (#2150)")
ok('settings["hibernate_ready"]' in _t and "#2150" in _t,
   "veille prolongée masquée quand elle n'est pas prête : testée sous Xvfb par test-power-page.py (#2150)")
