# Carte « Ces applications empêchent l'arrêt » : « Annuler » par défaut ; sur
# un écran verrouillé, la touche qui réveille l'écran éteignait l'ordinateur (#2299).
_p = (ROOT / "scripts/winlux-power.py").read_text(encoding="utf-8")
_b = _p[_p.index("    def show_blocked(self"):_p.index("    def show_error(self")]
ok('[(_t("Annuler"), self.cancel, True),' in _b and "self.finish, False)" in _b, "carte d'arrêt bloquée : « Annuler » par défaut (#2299)")
ok((ROOT / "scripts/test-power-carte-annuler.py").exists(), "test du focus de la carte d'arrêt sous Xvfb (#2299)")
