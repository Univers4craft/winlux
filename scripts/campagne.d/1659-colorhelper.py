# #1659 : Windows.UI.ColorHelper pour l'installateur en ligne d'iCUE 5.
w = (runtime_dlls / "winrt_ui.rs").read_text(encoding="utf-8")
ok('"Windows.UI.ColorHelper"' in w and "fn from_argb(" in w and "fn to_display_name(" in w,
   "Windows.UI.ColorHelper : FromArgb et ToDisplayName servis (#1659)")
e = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("winrt_ui::activation_factory(" in e,
   "RoGetActivationFactory connaît Windows.UI.ColorHelper (#1659)")
