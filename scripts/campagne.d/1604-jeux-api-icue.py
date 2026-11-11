# #1604 : jeux d'API importés par l'installeur d'iCUE (ntuser-sysparams, shcore-obsolete).
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
obs = (runtime_dlls / "shcore_obsolete.rs").read_text(encoding="utf-8")
ok('base.starts_with("api-ms-win-ntuser-sysparams-")' in mod_rs, "api-ms-win-ntuser-sysparams servi par user32 (#1604)")
ok("shcore_obsolete::exports()" in mod_rs and '"CommandLineToArgvW" => fn CommandLineToArgvW' in obs
   and '"SHStrDupW" => fn SHStrDupW' in obs, "api-ms-win-shcore-obsolete : CommandLineToArgvW et SHStrDup (#1604)")
