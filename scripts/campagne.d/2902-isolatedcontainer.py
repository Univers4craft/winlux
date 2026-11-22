# Contrats api-ms-win-security-isolatedcontainer-l1-1-0/-l1-1-1 servis :
# un processus Winlux n'est jamais dans un conteneur isolé (Refs #2902).
_i = (runtime_dlls / "isolatedcontainer.rs").read_text(encoding="utf-8")
_m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('name: "api-ms-win-security-isolatedcontainer-l1-1-0"' in _m
   and all(f'"{n}" => fn' in _i for n in ("IsProcessInIsolatedContainer", "IsProcessInWDAGContainer")),
   "les deux fonctions des contrats isolatedcontainer sont servies (#2902)")
ok("fn jamais_dans_un_conteneur_isole_2902" in _i,
   "test : réponse « pas dans un conteneur isolé » et résolution des deux versions du contrat (#2902)")
