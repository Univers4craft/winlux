# FindFirstFile accepte « \\?\C:\…\* » : Steam voit ses fichiers extraits et finit sa mise à jour (#1475).
ok("fn prefixe_chemin_long_avec_joker" in (ROOT / "runtime/src/dlls/winfile.rs").read_text(encoding="utf-8"),
   "FindFirstFile avec le préfixe \\\\?\\ et un joker (#1475)")
