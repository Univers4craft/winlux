# #2207 : UISettings sert IWeakReferenceSource ; Teams plantait sur la
# référence faible prise par un abonnement auto_revoke.
ui = (ROOT / "runtime" / "src" / "dlls" / "ui_settings.rs").read_text(encoding="utf-8")
wr = (ROOT / "runtime" / "src" / "dlls" / "weak_ref.rs").read_text(encoding="utf-8")
ok("IID_IWEAK_REFERENCE_SOURCE" in ui and "fn get_weak_reference" in ui
   and "fn w_resolve" in wr and "0x0000_0038" in wr,
   "WinRT : UISettings rend IWeakReferenceSource et une référence faible (#2207)")
