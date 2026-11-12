# Runtime VC++ intégré : msvcp140_1 (std::pmr) pour Qt6Core / OBS sans redistribuable (#2178).
_m = (ROOT / "runtime/src/dlls/mod.rs").read_text()
_r = _m[_m.index("pub fn prefers_real_file("):]
_r = _r[:_r.index("\n}\n")]
ok('name: "msvcp140_1"' in _m and '"msvcp140_1"' in _r,
   "msvcp140_1 intégrée, la vraie DLL passe avant quand elle est présente (#2178)")
_p = (ROOT / "runtime/src/dlls/msvcp140_1.rs").read_text()
ok(all(f'"{n}"' in _p for n in ("_Aligned_get_default_resource", "_Aligned_set_default_resource",
       "_Aligned_new_delete_resource", "_Unaligned_get_default_resource",
       "_Unaligned_set_default_resource", "_Unaligned_new_delete_resource", "null_memory_resource")),
   "msvcp140_1 : les sept fonctions de memory_resource exportées (#2178)")
