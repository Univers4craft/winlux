# GetProcAddress(KernelBase, …) trouve les fonctions de sécurité et de
# registre qu'advapi32 sert et que KernelBase exporte sous Windows, comme
# GetTokenInformation cherchée par le courtier d'Acrobat (#3207).
mod_3207 = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
ok("fn kernelbase_exporte_la_securite_d_advapi32_3207" in mod_3207,
   "KernelBase : GetTokenInformation et la sécurité d'advapi32 trouvées par GetProcAddress (#3207)")
