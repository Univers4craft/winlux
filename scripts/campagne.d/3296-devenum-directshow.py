# L'énumérateur système de DirectShow (CLSID_SystemDeviceEnum, ICreateDevEnum)
# manquait : Discord et tout programme listant ses caméras par DirectShow
# recevaient REGDB_E_CLASSNOTREG (#3296). Le test unitaire nommé ici vérifie la
# sémantique de Learn : catégorie vide → S_FALSE et énumérateur NULL.
_qz = (ROOT / "runtime" / "src" / "dlls" / "quartz.rs").read_text(encoding="utf-8")
ok("fn enumerateur_systeme_directshow_categorie_vide_3296" in _qz,
   "ICreateDevEnum servi, catégorie vide en S_FALSE (#3296)")
