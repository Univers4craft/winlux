# Actions personnalisées DLL d'Acrobat (#1757) : msi.dll #211, DeleteMonitorW,
# INSERT … TEMPORARY, ITaskService en x86, PostMessage(HWND_BROADCAST).
_leg = (ROOT / "runtime/src/dlls/legacy_dlls.rs").read_text(encoding="utf-8")
ok('"MsiSourceListAddSourceW" => fn msi_source_list_ok' in _leg,
   "msi.dll : MsiSourceListAddSourceW (#211) servie (#1757)")
_spool = (ROOT / "runtime/src/dlls/lot_e_divers.rs").read_text(encoding="utf-8")
ok('"DeleteMonitorW" => fn DeleteMonitorW' in _spool, "winspool : DeleteMonitorW servie (#1757)")
_db = (ROOT / "runtime/src/msi/session_db.rs").read_text(encoding="utf-8")
ok("fn parse_insert(" in _db and "fn insert_temporaire_comme_acrobat" in _db,
   "MSI : INSERT … TEMPORARY dans les vues des actions (#1757)")
_com = (ROOT / "runtime/src/wow32/com.rs").read_text(encoding="utf-8")
ok("com_taskschd::ALL" in _com, "ITaskService décrit pour le code x86 (#1757)")
_u = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("if hwnd == HWND_BROADCAST {" in _u, "PostMessage(HWND_BROADCAST) réussit (#1757)")
