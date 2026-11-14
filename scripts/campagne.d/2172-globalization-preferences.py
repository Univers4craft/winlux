# #2172 : Windows.System.UserProfile.GlobalizationPreferences servie
# (Teams s'arrêtait sur son REGDB_E_CLASSNOTREG relancé).
ro = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
gp = (ROOT / "runtime" / "src" / "dlls" / "globalization_prefs.rs").read_text(encoding="utf-8")
ok("globalization_prefs::activation_factory" in ro
   and '"Windows.System.UserProfile.GlobalizationPreferences"' in gp
   and "0x01BF_4326" in gp,
   "WinRT : GlobalizationPreferences a sa fabrique (#2172)")
