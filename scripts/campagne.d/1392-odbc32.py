# #1392 : odbc32 intégrée, fonctions SQL* par nom et par ordinal, réponse « aucun pilote ».
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
odbc = (runtime_dlls / "odbc32.rs").read_text(encoding="utf-8")
ok('name: "odbc32"' in mod_rs, "odbc32.dll intégrée (#1392)")
ok('(141, "SQLDriverConnect")' not in odbc and '(41, "SQLDriverConnect")' in odbc and "ordinal + 100" in odbc,
   "odbc32 : table des ordinaux, variantes W à +100 et A à +200 (#1392)")
ok('"IM002"' in odbc, "odbc32 : une connexion échoue proprement avec IM002 (#1392)")
