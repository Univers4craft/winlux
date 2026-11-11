# Boîte de dialogue MFC : QueryActCtxW réussit et InitNetworkAddressControl existe (installeur d'IrfanView, #1685).
act = (ROOT / "runtime/src/dlls/actctx.rs").read_text(encoding="utf-8")
sh = (ROOT / "runtime/src/dlls/shell32.rs").read_text(encoding="utf-8")
ctl = (ROOT / "runtime/src/dlls/controls.rs").read_text(encoding="utf-8")
ok("ACTIVATION_CONTEXT_BASIC_INFORMATION" in act and "(#1685)" in act,
   "QueryActCtxW : contexte vide décrit au lieu d'un échec (#1685)")
ok('"InitNetworkAddressControl"' in sh and '"msctls_netaddress"' in ctl,
   "shell32 : InitNetworkAddressControl et classe msctls_netaddress (#1685)")
