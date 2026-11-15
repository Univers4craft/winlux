# #2189 : la désinstallation retire ce que l'installation a apporté et jamais
# les compagnons d'un autre logiciel ; comportement vérifié par test-store.py
# (scénarios vlc et knights) en CI.
_s = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
_t = (SCRIPTS / "test-store.py").read_text(encoding="utf-8")
ok("def autoremovable(" in _s and "def all_companions(" in _s,
   "Boutique : calcul des paquets devenus inutiles et des compagnons protégés (#2189)")
ok("compagnon d'un logiciel restant jamais retiré (#2189)" in _t,
   "Boutique : désinstallation sans casser un autre logiciel testée (#2189)")
