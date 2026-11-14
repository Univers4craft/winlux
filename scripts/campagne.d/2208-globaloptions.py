# #2208 : CoCreateInstance(CLSID_GlobalOptions) sert IGlobalOptions
# (Teams s'arrêtait sur REGDB_E_CLASSNOTREG).
ole = (ROOT / "runtime" / "src" / "dlls" / "ole32.rs").read_text(encoding="utf-8")
go = (ROOT / "runtime" / "src" / "dlls" / "global_options.rs").read_text(encoding="utf-8")
ok("super::global_options::" in ole and "CLSID_GLOBAL_OPTIONS" in go and "IID_IGLOBAL_OPTIONS" in go,
   "COM : CLSID_GlobalOptions et IGlobalOptions servis (#2208)")
