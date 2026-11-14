# #2206 : Windows.UI.ViewManagement.UISettings activable (Teams s'arrêtait
# sur son REGDB_E_CLASSNOTREG non rattrapé).
ro = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
ui = (ROOT / "runtime" / "src" / "dlls" / "ui_settings.rs").read_text(encoding="utf-8")
ok("ui_settings::activation_factory" in ro
   and '"Windows.UI.ViewManagement.UISettings"' in ui
   and "fn activate_instance" in ui,
   "WinRT : UISettings a sa fabrique et ActivateInstance (#2206)")
