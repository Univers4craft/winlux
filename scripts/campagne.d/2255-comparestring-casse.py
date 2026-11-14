# #2255 : CompareStringA/W appliquent dwCmpFlags (winnls.h) ; sans
# NORM_IGNORECASE la comparaison tient compte de la casse.
k32 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
ok("super::nls::compare(a, b, flags)" in k32, "CompareStringA/W appliquent dwCmpFlags (#2255)")
ok("fn compare_string_respecte_la_casse_sans_ignorecase" in k32, "test de la casse de CompareStringW (#2255)")
