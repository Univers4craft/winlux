# Contrat api-ms-win-security-systemfunctions-l1-* rattaché à advapi32 :
# Audacity 4.0.2 l'importe pour SystemFunction036 (#2902).
_m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('base.starts_with("api-ms-win-security-systemfunctions-l1-")' in _m,
   "le contrat security-systemfunctions est servi par advapi32 (#2902)")
ok(all(f'"SystemFunction0{n}",' in _m for n in ("36", "40", "41")),
   "SystemFunction036, 040 et 041 se résolvent par le contrat dans les tests (#2902)")
