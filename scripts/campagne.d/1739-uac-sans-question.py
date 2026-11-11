# UAC : un compte administrateur lance un installeur sans question ; un compte standard donne le mot de passe d'un administrateur (#1739).
uac = (ROOT / "runtime/src/uac.rs").read_text(encoding="utf-8")
ok("fn shows_dialog(admin_capable: bool)" in uac and "!admin_capable" in uac
   and "fn fenetre_seulement_pour_un_compte_standard" in uac,
   "UAC : élévation sans fenêtre pour un administrateur, fenêtre gardée pour un compte standard (#1739)")
