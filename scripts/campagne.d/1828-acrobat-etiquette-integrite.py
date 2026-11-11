# #1828 GetKernelObjectSecurity rend la SACL et l'étiquette d'intégrité demandées.
s = (runtime_dlls / "security.rs").read_text(encoding="utf-8")
ok("fn with_integrity_label(" in s and "SACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION" in s,
   "GetKernelObjectSecurity rend une SACL avec l'étiquette d'intégrité du processus (#1828)")
