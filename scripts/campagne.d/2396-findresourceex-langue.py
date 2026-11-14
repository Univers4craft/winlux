# FindResourceEx : la ressource dans la langue demandée, 1815 si elle manque (#2396).
_r = (ROOT / "runtime/src/dlls/resource.rs").read_text()
ok("fn choose_language" in _r and "fn find_resource_ex_langue_demandee" in _r,
   "FindResourceEx rend la langue demandée (#2396)")
