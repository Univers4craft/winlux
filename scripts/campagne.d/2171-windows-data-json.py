# #2171 : Spotify crée un Windows.Data.Json.JsonObject dès son démarrage.
s = (runtime_dlls / "data_json.rs").read_text(encoding="utf-8")
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok("pub mod data_json;" in m
   and 'const CLASS_OBJECT: &str = "Windows.Data.Json.JsonObject";' in s
   and "fn json_object_par_la_fabrique(" in s,
   "WinRT : Windows.Data.Json.JsonObject a sa fabrique, testée (#2171)")
ok('const CLASS_ARRAY: &str = "Windows.Data.Json.JsonArray";' in s
   and "fn json_array_et_parse(" in s,
   "WinRT : JsonArray et son analyse, testés (#2171)")
ok("WEB_E_JSON_VALUE_NOT_FOUND" in s and "fn analyse_et_ecriture(" in s,
   "JsonValue et les erreurs WEB_E_* documentées, testées (#2171)")
