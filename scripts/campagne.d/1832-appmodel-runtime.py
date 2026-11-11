# #1832 api-ms-win-appmodel-runtime-* servi par kernel32 (Spotify.dll, Acrobat).
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
k = (runtime_dlls / "kernel32_misc.rs").read_text(encoding="utf-8")
ok('base.starts_with("api-ms-win-appmodel-runtime-l1-")' in m,
   "api-ms-win-appmodel-runtime-* redirigé vers kernel32 (#1832)")
ok(all(f'"{n}"' in k for n in ("GetPackageFullName", "GetCurrentPackageInfo", "PackageFamilyNameFromFullName", "PackageIdFromFullName")),
   "kernel32 sert le reste du contrat appmodel-runtime (#1832)")
