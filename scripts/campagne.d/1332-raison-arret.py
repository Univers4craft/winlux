# ShutdownBlockReasonCreate/Destroy/Query exportées : les chargeurs
# PyInstaller les importent directement (#1332).
ok("\"ShutdownBlockReasonCreate\"=>fn shutdown_block_reason_create" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "fn raison_de_blocage_de_l_arret" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8"),
   "ShutdownBlockReason* pour PyInstaller (#1332)")
