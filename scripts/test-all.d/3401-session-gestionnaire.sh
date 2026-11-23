# #3401 : la session d'un second compte s'arrête en le disant quand openbox
# est mort ou vivant mais muet, au lieu de s'ouvrir sans gestionnaire.
step "Session d'un second compte : gestionnaire de fenêtres mort ou muet signalé (#3401)" sh scripts/test-user-session-gestionnaire.sh
