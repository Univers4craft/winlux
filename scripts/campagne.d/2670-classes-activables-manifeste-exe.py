# #2670 : RoGetActivationFactory sert aussi les classes activableClass du
# manifeste d'application de l'exe (activation WinRT sans inscription).
p = (ROOT / "runtime/src/dlls/package_classes.rs").read_text(encoding="utf-8")
ok("fn classes_du_manifeste_d_application_2670" in p,
   "WinRT : classes du manifeste d'application de l'exe servies (#2670)")
