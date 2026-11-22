# Un contrôle enfant mis en hauteur 0 garde une zone client vide : l'écho X
# (1 pixel au minimum) ne remonte pas au programme (#2992).
ok((ROOT / "testexe" / "src" / "bin" / "libelle_vide_suite.rs").exists(), "Suite PE du contrôle enfant en hauteur 0 présente (#2992)")
ok((ROOT / "scripts" / "test-libelle-vide.sh").exists(), "Contrôle enfant en hauteur 0 sous X : aucun WM_PAINT à zone vide (#2992)")
