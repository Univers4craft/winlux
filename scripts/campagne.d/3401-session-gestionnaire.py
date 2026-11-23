# La session d'un second compte attend son gestionnaire de fenêtres et
# s'arrête en le disant s'il ne répond pas, au lieu de s'ouvrir sans lui (#3401).
ok((ROOT / "scripts/test-user-session-gestionnaire.sh").exists(),
   "Test de la session sans réponse du gestionnaire de fenêtres (#3401)")
