# #2122 : Teams lit Windows.Storage.ApplicationData.Current au démarrage.
s = (runtime_dlls / "storage_appdata.rs").read_text(encoding="utf-8")
e = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok('const CLASS: &str = "Windows.Storage.ApplicationData";' in s
   and "storage_appdata::activation_factory" in e,
   "RoGetActivationFactory sert Windows.Storage.ApplicationData (#2122)")
ok(".mode(0o700)" in s and "from_mode(0o700)" in s,
   "dossiers de paquet réservés au compte, 0700 (#2122)")
ok('"GetCurrentPackageFamilyName"' in s and "E_NO_PACKAGE" in s,
   "chemin dérivé de la famille du paquet, APPMODEL_ERROR_NO_PACKAGE hors paquet (#2122)")
