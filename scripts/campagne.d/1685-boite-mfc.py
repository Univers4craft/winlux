# Boîte de dialogue MFC : QueryActCtxW réussit et InitNetworkAddressControl existe (installeur d'IrfanView, #1685).
act = (ROOT / "runtime/src/dlls/actctx.rs").read_text(encoding="utf-8")
sh = (ROOT / "runtime/src/dlls/shell32.rs").read_text(encoding="utf-8")
ctl = (ROOT / "runtime/src/dlls/controls.rs").read_text(encoding="utf-8")
ok("ACTIVATION_CONTEXT_BASIC_INFORMATION" in act and "(#1685)" in act,
   "QueryActCtxW : contexte vide décrit au lieu d'un échec (#1685)")
ok('"InitNetworkAddressControl"' in sh and '"msctls_netaddress"' in ctl,
   "shell32 : InitNetworkAddressControl et classe msctls_netaddress (#1685)")
rel = (ROOT / "runtime/src/dlls/user32_releve.rs").read_text(encoding="utf-8")
dlg = (ROOT / "runtime/src/dlls/dialogs.rs").read_text(encoding="utf-8")
ok(all(f'"{n}"' in rel for n in ("GrayStringW", "GetMenuCheckMarkDimensions",
                                  "RealChildWindowFromPoint", "ScaleViewportExtEx")),
   "imports MFC de l'installeur IrfanView présents (#1685)")
ok("style & 0x8 != 0" not in dlg and "modèle illisible" in dlg,
   "modèle de boîte : DS_FIXEDSYS seul sans police, échec de création tracé (#1685)")
