# #1317 : GetData d'une requête TIMESTAMP_DISJOINT rend une fréquence non nulle.
ok("TIMESTAMP_FREQUENCY" in (runtime_dlls / "d3d11_query.rs").read_text(encoding="utf-8")
   and "get_data: query::ctx_get_data" in (runtime_dlls / "d3d11.rs").read_text(encoding="utf-8")
   and "create_query: dev_create_query" in (runtime_dlls / "d3d11.rs").read_text(encoding="utf-8"),
   "Requêtes Direct3D 11 d'horodatage (#1317)")
