# #2657 : RoGetActivationFactory sert les classes que le manifeste du paquet
# MSIX déclare (windows.activatableClass.inProcessServer), par le
# DllGetActivationFactory de la DLL du paquet.
m = (ROOT / "runtime/src/setup/msix.rs").read_text(encoding="utf-8")
p = (ROOT / "runtime/src/dlls/package_classes.rs").read_text(encoding="utf-8")
ok("fn classes_activables_du_paquet_2657" in m
   and "fn fabrique_du_paquet_par_dll_get_activation_factory_2657" in p,
   "WinRT : classes activables du paquet servies (#2657)")
