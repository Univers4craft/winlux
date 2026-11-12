# #2110 : 38 fonctions importées par ms-teams.exe et ses DLL.
t = (runtime_dlls / "teams_win32.rs").read_text(encoding="utf-8")
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
p = (runtime_dlls / "threadpool.rs").read_text(encoding="utf-8")
c = (runtime_dlls / "common_apps.rs").read_text(encoding="utf-8")
ok(".chain(teams_win32::all())" in m, "imports de Teams servis par teams_win32 (#2110)")
ok('"ShowWindowAsync" => fn ShowWindowAsync' in t and '"IsGUIThread" => fn IsGUIThread' in t,
   "ShowWindowAsync et IsGUIThread servies (#2110)")
ok('"CreateThreadpoolCleanupGroup"' in p and '"SetThreadpoolTimerEx"' in p,
   "groupes de nettoyage du pool et SetThreadpoolTimerEx (#2110)")
ok('("shlwapi", 12, "SHCreateMemStream")' in c and '("oleaut32", 27, "SafeArrayCopy")' in c,
   "numéros d'ordre shlwapi et oleaut32 de Teams (#2110)")
