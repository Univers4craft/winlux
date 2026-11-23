# Une fenêtre affichée pendant le démarrage d'openbox n'attend plus le
# prochain événement X : les deux scripts de session lancent winlux-wm-reveil
# avec le gestionnaire, et l'image le livre (#3190).
for nom in ("winlux-user-session.sh", "winlux-native-session.sh"):
    session_3190 = (ROOT / "scripts" / nom).read_text(encoding="utf-8")
    ok("/usr/bin/winlux-wm-reveil" in session_3190,
       f"{nom} lance winlux-wm-reveil avec le gestionnaire de fenêtres (#3190)")
stage_3190 = (ROOT / "scripts/stage-desktop-components.sh").read_text(encoding="utf-8")
ok("winlux-wm-reveil.sh:winlux-wm-reveil" in stage_3190,
   "winlux-wm-reveil livré dans l'image (#3190)")
ok((ROOT / "scripts/test-wm-reveil.py").exists(),
   "Test d'une fenêtre affichée au démarrage d'openbox (#3190)")
