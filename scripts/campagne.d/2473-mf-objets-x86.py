# Objets de base de Media Foundation pour les programmes x86 (#2473, famille
# #2428). Le comportement est vérifié par mf32.exe et mf64.exe dans
# scripts/test-wow32.sh et par les tests unitaires nommés ci-dessous ; ce
# contrôle vérifie les noms.
_m = (ROOT / "runtime/src/wow32/com_mf.rs").read_text()
_c = (ROOT / "runtime/src/wow32/com.rs").read_text()
_t = (ROOT / "scripts/test-wow32.sh").read_text()
ok("static IMFATTRIBUTES:" in _m and "static IMFMEDIAEVENTQUEUE:" in _m
   and "super::com_mf::ALL" in _c and "super::com_mf::api_kinds" in _c,
   "interfaces et exports MF décrits pour les programmes x86 (#2473)")
ok("fn interfaces_completes()" in _m and "fn exports_decrits()" in _m,
   "tests : interfaces MF complètes et exports décrits (#2473)")
ok("mf32 mf64" in _t and (ROOT / "testc/wow32/mf32.c").exists(),
   "test-wow32 : mf32.exe et mf64.exe (#2473)")
