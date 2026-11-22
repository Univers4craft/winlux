# #2904 : « Redémarrer maintenant » en fin d'installation, média éjecté. En
# session live tout le système vient du squashfs du média : l'installateur
# root signale directement l'init (SIGTERM = redémarrage) sans rien lancer
# depuis le média. Comportement gardé par scripts/test-power-reboot-guard.py
# (cas « Session live (root, init shell) »), lancé par la CI.
garde_2904 = (ROOT / "scripts" / "test-power-reboot-guard.py").read_text(encoding="utf-8")
ci_2904 = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
ok("(#2904)" in garde_2904 and "Session live (root, init shell)" in garde_2904
   and "scripts/test-power-reboot-guard.py" in ci_2904,
   "Installateur : redémarrage sans relire le média éjecté (#2904)")
