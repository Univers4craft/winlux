# #3289 : ThreadPool.SetMinThreads retient les valeurs que GetMinThreads rend,
# et rend false hors des bornes, au lieu de rendre true sans rien faire. Le
# comportement est gardé par le témoin nommé, lancé par scripts/test-bcl.sh.
t = ROOT / "bcl" / "temoins" / "PoolMinimum.cs"
texte = t.read_text(encoding="utf-8") if t.exists() else ""
ok("Code de sortie attendu : 200." in texte, "témoin PoolMinimum : SetMinThreads retenu et borné (#3289)")
