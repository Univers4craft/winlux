# #2255 : CompareStringA/W appliquent dwCmpFlags (winnls.h) ; sans
# NORM_IGNORECASE la comparaison tient compte de la casse.
# Le comportement est gardé par le test unitaire nommé ci-dessous, que la CI
# exécute ; ce contrôle ne vérifie que des noms (#2349).
k32 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
ok('"CompareStringW"=>fn compare_string_w' in k32, "CompareStringW exporté par compare_string_w (#2255)")
ok("fn compare_result(" in k32, "CompareStringA/W passent par compare_result (#2255)")
ok("fn compare_string_respecte_la_casse_sans_ignorecase" in k32, "test de la casse de CompareStringW (#2255)")
