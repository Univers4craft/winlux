# #2274 : CLSIDFromString résout un ProgID (Remarks de la documentation
# publique) ; GUID_NULL en cas d'échec (règle à nous) ; IIDFromString à part.
# Le comportement est gardé par le test unitaire nommé ci-dessous, que la CI
# exécute ; ce contrôle ne vérifie que des noms (#2349).
src = (ROOT / "runtime" / "src" / "dlls" / "ole32.rs").read_text(encoding="utf-8")
ok('"CLSIDFromString"=>fn clsid_from_string' in src, "CLSIDFromString exporté par clsid_from_string (#2274)")
ok('"IIDFromString"=>fn iid_from_string' in src, "IIDFromString garde l'analyse seule (#2274)")
ok("fn clsid_from_string_resout_un_progid" in src, "test unitaire ProgID de CLSIDFromString (#2274)")
