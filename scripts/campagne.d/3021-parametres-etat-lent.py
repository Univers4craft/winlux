# Paramètres › Imprimantes et scanners (26) et Partage et transfert (28)
# s'affichent sans attendre lpstat ni winlux-control : l'état lent est lu
# hors du fil GTK puis la page se complète (#3021), testé sous Xvfb en CI.
_t = (SCRIPTS / "test-settings-etat-lent.py").read_text(encoding="utf-8")
ok("def status_later(" in (SCRIPTS / "winlux-settings-pages.py").read_text(encoding="utf-8")
   and "s'ouvre sans attendre lpstat" in _t and "s'ouvre sans attendre winlux-control" in _t
   and "test-settings-etat-lent.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
   and "test-settings-etat-lent.py" in (SCRIPTS / "test-all.d/3021-parametres-etat-lent.sh").read_text(encoding="utf-8"),
   "Paramètres : pages Imprimantes et Partage affichées sans attendre leur état lent (#3021)")
