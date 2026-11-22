# #2996 : la session d'un compte (winlux-user-session, systèmes installés)
# précharge le menu Démarrer comme la session native ; seule cette dernière
# le faisait, et le premier appui lançait Python et GTK à froid. Le
# préchargement lui-même est mesuré sous Xvfb par test-start-menu-preload.py.
_sessions_2996 = {nom: (SCRIPTS / nom).read_text(encoding="utf-8")
                  for nom in ("winlux-user-session.sh", "winlux-native-session.sh")}
ok(all("exec /usr/lib/winlux/winlux-start-menu --preload" in texte for texte in _sessions_2996.values()),
   "Démarrer préchargé dans la session du compte comme dans la session native (#2996)")
