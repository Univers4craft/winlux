# Chaque DLL reçoit DllMain après celles qu'elle importe (Steam 64 bits : tier0 avant vstdlib) (#1584).
ok("fn dllmain_des_dependances_dans_l_ordre_des_imports" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "dependency_order(&exe, deps)" in (ROOT / "runtime/src/process.rs").read_text(encoding="utf-8"),
   "DllMain des dépendances dans l'ordre des imports, au lancement et à LoadLibrary (#1584)")
