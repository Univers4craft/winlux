# #2056 : pas d'icu.dll partielle, .NET (Paint.NET) retombe sur NLS.
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('name: "icuuc"' in m and 'name: "icu",' not in m,
   "icu.dll n'est plus servie, seule icuuc.dll l'est (#2056)")
