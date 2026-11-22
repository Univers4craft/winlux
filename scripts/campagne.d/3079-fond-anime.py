# Fond « Winlux Aurore » animé (#3079) : désactivé par défaut, réglage visible
# dans Paramètres, coupé dès qu'on ne le voit pas. Le comportement est vérifié
# sous Xvfb par test-fond-anime.py, lancé par la CI ; on garde ici ses
# vérifications nommées et le réglage affiché.
_t = (ROOT / "scripts/test-fond-anime.py").read_text(encoding="utf-8")
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
_p = (ROOT / "scripts/winlux-settings-pages.py").read_text(encoding="utf-8")
for _nom in ("désactivé par défaut, rien ne tourne (#3079)",
             "sans compositeur, l'animation ne se lance pas (#3079)",
             "la couche laisse passer les clics vers la fenêtre du dessous (#3079)",
             "fenêtre plein écran : aucune image dessinée (#3079)",
             "fenêtre agrandie : aucune image dessinée (#3079)",
             "économiseur d'écran actif : aucune image dessinée (#3079)",
             "batterie à 15 % sur batterie : aucune image dessinée (#3079)",
             "autre fond (Aurore claire) : l'animation s'arrête d'elle-même (#3079)",
             "compositeur arrêté : l'animation s'arrête d'elle-même (#3079)",
             "désactivé dans Paramètres : l'animation s'arrête (#3079)"):
    ok(_nom in _t, "test du fond animé : " + _nom)
ok("scripts/test-fond-anime.py" in _ci, "la CI lance le test du fond animé (#3079)")
ok("def animated_wallpaper" in _p and '"Animer le fond Winlux Aurore": (' in _p,
   "Paramètres › Personnalisation affiche l'interrupteur du fond animé, traduit (#3079)")
ok((ROOT / "assets/wallpapers/winlux-prisme-aurore.etoiles").is_file(),
   "étoiles du fond Aurore livrées pour l'animation (#3079)")
