# Famille PathCch (api-ms-win-core-path) : Paint.NET 5.2 importe
# PathCchRemoveFileSpec (#2921).
_p = (runtime_dlls / "pathcch.rs").read_text(encoding="utf-8")
_m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('"PathCchRemoveFileSpec" => fn path_cch_remove_file_spec' in _p and "e.extend(pathcch::exports());" in _m,
   "PathCchRemoveFileSpec servie par le contrat api-ms-win-core-path (#2921)")
ok(all(f'"{n}",' in _m for n in ("PathCchRemoveFileSpec", "PathCchIsRoot", "PathIsUNCEx")),
   "la famille PathCch reste invisible par GetProcAddress sur kernel32, comme sous Windows (#2921)")
ok(all(f"fn {t}_2921" in _p for t in ("retire_le_dernier_element", "racines_de_la_documentation", "servies_par_le_contrat_path")),
   "tests des tables de la documentation : élément retiré, racines, contrat (#2921)")
