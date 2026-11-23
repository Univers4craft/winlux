# Les exports par ordinal des DLL intégrées sont à leur numéro dans la table
# d'exports de l'image, pas écrits comme des noms « #N » (#3424).
_pe = (ROOT / "runtime" / "src" / "dlls" / "pe_image.rs").read_text(encoding="utf-8")
print("tests présents :", "fn ordinaux_publies_a_leur_numero_dans_la_table_3424" in _pe,
      "fn shlwapi_ordinal_12_lu_dans_l_image_3424" in _pe)
ok("fn ordinaux_publies_a_leur_numero_dans_la_table_3424" in _pe
   and "fn shlwapi_ordinal_12_lu_dans_l_image_3424" in _pe and "fn layout(" in _pe,
   "table d'exports des DLL intégrées : ordinaux publiés à leur numéro (#3424)")
