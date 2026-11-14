# #1638 : imports non bloquants de libcef (ws2_32, userenv, secur32, wintrust, oleaut32 #185/#314).
# Les WTHelper* vivent dans wintrust.rs depuis #1728, CreateAppContainerProfile
# dans electron_win32.rs avec les autres fonctions AppContainer depuis #2495.
d = "".join((runtime_dlls / f).read_text(encoding="utf-8") for f in ("lot_e_divers.rs", "wintrust.rs", "electron_win32.rs"))
ok(all(f'"{n}" => fn' in d for n in ("WSCEnumProtocols", "WSCGetProviderPath", "CreateAppContainerProfile",
   "EnterCriticalPolicySection", "RegisterGPNotification", "LsaLogonUser", "WTHelperProvDataFromStateData",
   "WTHelperGetProvSignerFromChain", "WTHelperGetProvCertFromChain")),
   "imports de libcef servis : WSC*, stratégie de groupe, LsaLogonUser, WTHelper* (#1638)")
ca = (runtime_dlls / "common_apps.rs").read_text(encoding="utf-8")
ok('("oleaut32", 185, "VariantTimeToSystemTime")' in ca and '("oleaut32", 314, "VarBstrCmp")' in ca,
   "oleaut32 #185 et #314 lus dans OleAut32.Lib du SDK public (#1638)")
