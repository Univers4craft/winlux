# Contrats oobe-notification (kernel32) et gaming-expandedresources
# (gamemode) servis : accueil toujours terminé, jamais en mode Jeu (Refs #2902).
_o = (runtime_dlls / "oobe.rs").read_text(encoding="utf-8")
_g = (runtime_dlls / "gamemode.rs").read_text(encoding="utf-8")
ok("fn accueil_toujours_termine_2902" in _o and '"OOBEComplete" => fn' in _o,
   "OOBEComplete répond que l'accueil de Windows est terminé (#2902)")
ok("fn jamais_en_mode_jeu_2902" in _g and '"HasExpandedResources" => fn' in _g,
   "HasExpandedResources répond qu'aucune ressource exclusive n'est accordée (#2902)")
