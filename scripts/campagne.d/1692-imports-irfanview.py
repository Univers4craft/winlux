# #1692 : imports d'IrfanView 4.76.
u = (runtime_dlls / "user32_releve.rs").read_text(encoding="utf-8")
a = (runtime_dlls / "apps_win32.rs").read_text(encoding="utf-8")
m = (runtime_dlls / "gdi32_more.rs").read_text(encoding="utf-8")
ok('"SetWindowContextHelpId" => fn' in u and '"GetWindowContextHelpId" => fn' in u,
   "Set/GetWindowContextHelpId servies (#1692)")
ok('"SHGetDataFromIDListW" => fn' in a, "SHGetDataFromIDListW servie (#1692)")
ok('"GetMetaFileW" => fn' in m, "GetMetaFileW servie (#1692)")
