# #2194 : ApplicationData.LocalSettings et RoamingSettings rendent un
# ApplicationDataContainer dont les Values sont gardées d'un lancement à
# l'autre. Le comportement est gardé par les tests unitaires nommés.
_ad = (runtime_dlls / "storage_appdata.rs").read_text(encoding="utf-8")
_st = (runtime_dlls / "storage_settings.rs").read_text(encoding="utf-8")
ok("fn settings_getter(" in _ad
   and "fn dossier_settings_reserve_au_compte_2194()" in _ad
   and "fn reglages_locaux_gardes_d_un_lancement_a_l_autre_2194()" in _st,
   "WinRT : réglages LocalSettings/RoamingSettings enregistrés, tests (#2194)")
