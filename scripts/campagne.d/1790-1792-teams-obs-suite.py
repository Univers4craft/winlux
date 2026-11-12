# #1790, #1791, #1792 : contrat eventing-controller, WinVerifyTrustEx, DLL absente détaillée.
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
a = (runtime_dlls / "advapi32_releve.rs").read_text(encoding="utf-8")
w = (runtime_dlls / "wintrust.rs").read_text(encoding="utf-8")
l = (ROOT / "runtime/src/loader.rs").read_text(encoding="utf-8")
ok('base.starts_with("api-ms-win-eventing-controller-l1-")' in m and '"EnableTraceEx2" => fn enable_trace' in a,
   "api-ms-win-eventing-controller est servi par advapi32 (#1790)")
ok('"WinVerifyTrustEx" => fn WinVerifyTrustEx' in w,
   "wintrust sert WinVerifyTrustEx (#1791)")
ok("importée par {me}" in l and "fn import_names(" in l,
   "une DLL introuvable est signalée avec son importateur et ses fonctions (#1792)")
