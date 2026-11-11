# #1638 : imports non bloquants de libcef (ws2_32, userenv, secur32, wintrust, oleaut32 #185/#314).
d = (runtime_dlls / "lot_e_divers.rs").read_text(encoding="utf-8")
ok(all(f'"{n}" => fn' in d for n in ("WSCEnumProtocols", "WSCGetProviderPath", "CreateAppContainerProfile",
   "EnterCriticalPolicySection", "RegisterGPNotification", "LsaLogonUser", "WTHelperProvDataFromStateData",
   "WTHelperGetProvSignerFromChain", "WTHelperGetProvCertFromChain")),
   "imports de libcef servis : WSC*, stratégie de groupe, LsaLogonUser, WTHelper* (#1638)")
ca = (runtime_dlls / "common_apps.rs").read_text(encoding="utf-8")
ok('("oleaut32", 185, "VariantTimeToSystemTime")' in ca and '("oleaut32", 314, "VarBstrCmp")' in ca,
   "oleaut32 #185 et #314 lus dans OleAut32.Lib du SDK public (#1638)")
