# #2279 : CultureInfo.DefaultThreadCurrentCulture et
# DefaultThreadCurrentUICulture dans notre mscorlib ; lghub_setup.exe
# (G HUB) s'arrêtait sur MissingMethodException.
bcl = ROOT / "bcl" / "mscorlib"
ok("public static CultureInfo DefaultThreadCurrentCulture" in (bcl / "Membres.cs").read_text(encoding="utf-8")
   and "public static CultureInfo DefaultThreadCurrentUICulture" in (bcl / "Membres.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Cultures.cs").read_text(encoding="utf-8"),
   "CLR : CultureInfo.DefaultThreadCurrentCulture et DefaultThreadCurrentUICulture (#2279)")
