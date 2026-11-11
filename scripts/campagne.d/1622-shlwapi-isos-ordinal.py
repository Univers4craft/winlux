# shlwapi!IsOS servi aussi par l'ordinal 437 : le crochet de chargement différé de libcef (steamwebhelper) ne tombe plus sur int3 (#1622).
ok('"#437" => fn is_os' in (ROOT / "runtime/src/dlls/shlwapi.rs").read_text(encoding="utf-8"),
   "shlwapi exporte IsOS sous l'ordinal 437 (#1622)")
