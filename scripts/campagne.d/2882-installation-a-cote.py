# Installateurs : une installation du même nom ailleurs n'est pas nettoyée, et la question du script est posée (#2882).
_m = (ROOT / "runtime/src/setup/mod.rs").read_text()
_n = (ROOT / "runtime/src/setup/nsis.rs").read_text()
ok("fn reinstallation_seulement_au_meme_endroit" in _m,
   "la réinstallation ne nettoie que le même dossier d'installation (#2882)")
ok("fn boite_de_message_posee_a_l_utilisateur_present" in _n,
   "les boîtes de message NSIS sont montrées pendant une installation suivie (#2882)")
