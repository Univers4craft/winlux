# #2771 : IDispatch::Invoke passe la frontière x86 ↔ x64 avec ses DISPPARAMS
# convertis (16 octets et VARIANT de 16 octets en x86, 24 en x64), les
# VARIANT par référence revenant à l'appelant. Le contrôle WebBrowser
# prévient ainsi l'abonné x86 de l'installeur de Creative Cloud
# (DocumentComplete). Comportement gardé par le test
# wow32::com_disp::dispparams_aller_retour_x86.
disp_2771 = (ROOT / "runtime" / "src" / "wow32" / "com_disp.rs").read_text(encoding="utf-8")
com_2771 = (ROOT / "runtime" / "src" / "wow32" / "com.rs").read_text(encoding="utf-8")
ok("fn dispparams_aller_retour_x86" in disp_2771
   and "DispParams, Var, I, I]" in com_2771,
   "IDispatch::Invoke d'un programme x86 : DISPPARAMS convertis (#2771)")
