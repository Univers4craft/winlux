# #26 : DLL système importées en direct par le corpus de mesure.
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
for dll in ("authz", "avicap32", "credui", "evr", "ktmw32", "sfc", "loadperf", "rasapi32", "ntdsapi", "query"):
    ok(f'name: "{dll}"' in mod_rs, f"DLL système {dll} fournie (#26)")
ok('base == "imagehlp"' in mod_rs, "imagehlp servie par dbghelp (#26)")
ok((ROOT / "docs" / "compat" / "mesure-2026-11-09.md").exists(), "Mesure des imports du 9 novembre commitée (#26)")
