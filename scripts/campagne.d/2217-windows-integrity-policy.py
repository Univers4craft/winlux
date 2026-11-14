# #2217 : Windows.System.Profile.WindowsIntegrityPolicy servie (pas de mode S
# sous Winlux). Sans elle, le catch de Teams arrêtait le programme.
src = (ROOT / "runtime" / "src" / "dlls" / "integrity_policy.rs").read_text(encoding="utf-8")
disp = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
ok('const CLASS: &str = "Windows.System.Profile.WindowsIntegrityPolicy";' in src
   and "0x7D1D_81DB, 0x8D63, 0x4789" in src
   and "super::integrity_policy::activation_factory(" in disp,
   "WinRT : fabrique WindowsIntegrityPolicy servie (#2217)")
