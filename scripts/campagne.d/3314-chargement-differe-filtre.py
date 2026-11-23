# Chargement différé raté remis au filtre de l'appli (crashpad) : la ligne de
# diagnostic nomme la DLL et la fonction (#3314).
_seh = (ROOT / "runtime/src/dlls/seh.rs").read_text(encoding="utf-8")
ok("fn chargement_differe_rate_nomme_dans_la_ligne_du_filtre" in _seh,
   "exception remise au filtre de l'appli : chargement différé raté nommé (#3314)")
