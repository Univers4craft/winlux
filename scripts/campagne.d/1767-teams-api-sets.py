# #1767 : Teams importe api-ms-win-security-sddl et les jetons de processthreads.
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('base.starts_with("api-ms-win-security-sddl-l1-")' in m,
   "api-ms-win-security-sddl est servi par advapi32 (#1767)")
ok("KERNELBASE_SECURITY" in m and '"OpenProcessToken"' in m and '"OpenThreadToken"' in m,
   "OpenProcessToken/OpenThreadToken trouvées via api-ms-win-core-processthreads (#1767)")
