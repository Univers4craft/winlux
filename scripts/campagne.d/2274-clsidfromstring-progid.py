# #2274 : CLSIDFromString résout un ProgID (Remarks de la documentation
# publique) ; GUID_NULL en cas d'échec (règle à nous) ; IIDFromString à part.
src = (ROOT / "runtime" / "src" / "dlls" / "ole32.rs").read_text(encoding="utf-8")
ok("clsid_from_prog_id(value, out)" in src, "CLSIDFromString résout les ProgID (#2274)")
ok('"IIDFromString"=>fn iid_from_string' in src, "IIDFromString garde l'analyse seule (#2274)")
ok("fn clsid_from_string_resout_un_progid" in src, "test unitaire ProgID de CLSIDFromString (#2274)")
