# #3011 : une fenêtre à barre de titre sans WS_THICKFRAME ni WS_MAXIMIZEBOX
# (boîte de message, dialogue fixe) n'offre pas Agrandir. Le comportement
# est gardé par le test unitaire nommé et par scripts/test-cadre-fixe.sh
# (openbox réel sous Xvfb).
src = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
ok("fn cadre_fixe_sans_bouton_agrandir_3011" in src,
   "Fenêtre à cadre fixe sans bouton Agrandir (#3011)")
ok((ROOT / "scripts" / "test-cadre-fixe.sh").exists(),
   "Test openbox réel de la fenêtre à cadre fixe présent (#3011)")
