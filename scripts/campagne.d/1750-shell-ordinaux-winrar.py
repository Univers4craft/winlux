# shell32 #100 (SHRestricted), shlwapi #176 (IUnknown_QueryService) et #219 (QISearch) : WinRAR ne s'arrête plus sur 0xC0000139 (#1750).
_src = (ROOT / "runtime/src/dlls/shell_ordinaux.rs").read_text(encoding="utf-8")
ok('"#100" => fn sh_restricted' in _src, "shell32 exporte SHRestricted sous l'ordinal 100 (#1750)")
ok('"#176" => fn iunknown_query_service' in _src and '"#219" => fn qi_search' in _src,
   "shlwapi exporte IUnknown_QueryService (176) et QISearch (219) par ordinal (#1750)")
