# #3241 : sfnt::load ne garde pas pour tout le processus l'échec de lecture
# d'une police copiée ou réécrite ; gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/sfnt.rs").read_text(encoding="utf-8")
ok(
    "fn police_relue_apres_un_echec_quand_le_fichier_change_3241" in t,
    "une police illisible un instant redevient lisible une fois le fichier complet (#3241)",
)
