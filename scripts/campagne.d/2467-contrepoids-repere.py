# #2467 : barre centrée, le groupe Démarrer reste au milieu quand le bloc de
# droite commence par un greffon interne (bouton de l'assistant). Les
# comportements sont gardés par les tests nommés.
edge = SCRIPTS / "test-taskbar-balance-edge.py"
ok(edge.is_file() and "def ok(" in edge.read_text(encoding="utf-8"), "test du contrepoids mesuré depuis le repère 46 (#2467)")
ok("test-taskbar-balance-edge.py" in (SCRIPTS / "test-all.d/2467-contrepoids-repere.sh").read_text(encoding="utf-8"),
   "test du repère 46 lancé par test-all (#2467)")
ok("def add_assistant" in (SCRIPTS / "test-taskbar-boot-centering.py").read_text(encoding="utf-8"),
   "barre réelle relancée avec le bouton de l'assistant (#2467)")
