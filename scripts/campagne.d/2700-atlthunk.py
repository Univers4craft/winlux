# #2700 : atlthunk.dll est servie (thunks de fenêtre d'ATL 14, Windows 10
# et suivants). Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/atlthunk.rs").read_text(encoding="utf-8")
ok("fn thunk_atl_remplace_le_premier_argument" in t, "atlthunk.dll : thunk ATL servi (#2700)")
