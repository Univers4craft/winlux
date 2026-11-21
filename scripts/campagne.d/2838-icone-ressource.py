# WinForms : Icon(Type, nom) lit l'icône incorporée de l'assembly du type
# (GetManifestResourceStream, #2824) ; témoin IconeRessource (code 200, #2838).
_i = (ROOT / "bcl/mscorlib/Icones.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/IconeRessource.cs").read_text(encoding="utf-8")
ok("public Icon(Type type, string resource)" in _i and "Ressource incorporée : temoin.ico," in _t
   and (ROOT / "bcl/temoins/ressources/temoin.ico").is_file() and "Code de sortie attendu : 200." in _t,
   "WinForms : Icon(Type, nom) depuis une ressource incorporée, témoin (#2838)")
