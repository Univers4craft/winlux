# winlux-open lance le programme sans attendre l'enregistrement Python pour le
# menu Démarrer et ne l'inspecte qu'une fois ; la latence de la sonde Win32
# (premier pixel, touche, clic) est mesurée sous Xvfb (#2889).
_t = (SCRIPTS / "test-winlux-open-rapide.sh").read_text(encoding="utf-8")
ok("une seule inspection" in _t and "enregistrement de 3 s" in _t
   and (ROOT / "testc/latence/latence.c").exists()
   and (SCRIPTS / "mesure-latence.py").exists()
   and "test-latence-sonde.sh" in (SCRIPTS / "test-all.d/2889-latence-lancement.sh").read_text(encoding="utf-8"),
   "winlux-open : programme lancé sans attendre l'enregistrement, une seule inspection, latence mesurée (#2889)")
