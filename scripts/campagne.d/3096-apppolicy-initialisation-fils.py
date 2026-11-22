# kernel32 exporte AppPolicyGetThreadInitializationType : un programme de
# bureau n'a aucune initialisation WinRT à faire au début d'un fil (#3096).
k32_src = (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text(encoding="utf-8")
ok("fn politique_d_initialisation_des_fils_3096" in k32_src,
   "kernel32 : AppPolicyGetThreadInitializationType servie (#3096)")
