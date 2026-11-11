# Une vue FILE_MAP_ALL_ACCESS est partagée (le bit FILE_MAP_COPY y est SECTION_QUERY), et une vue retient la section nommée (#1707).
_m = (ROOT / "runtime/src/dlls/mmap.rs").read_text(encoding="utf-8")
ok("access & (FILE_MAP_WRITE | _FILE_MAP_READ) == 0" in _m and "if copy_on_write {" in _m,
   "copie sur écriture seulement pour FILE_MAP_COPY seul (#1707)")
ok(_m.count("named_objects::release(crate::dlls::named_objects::SECTION") >= 2
   and "fn section_nommee_vues_all_access_partagees_et_vivantes" in _m,
   "une vue garde la section nommée trouvable après CloseHandle (#1707)")
