# Objets WAM servis entiers : GetView, InvalidateCacheAsync, Properties (#2948).
o = (ROOT / "runtime" / "src" / "dlls" / "winrt_wam_objets.rs").read_text(encoding="utf-8")
ok("fn vue_proprietes_et_cache_2948" in o, "objets WAM servis entiers : vue, propriétés, cache (#2948)")
