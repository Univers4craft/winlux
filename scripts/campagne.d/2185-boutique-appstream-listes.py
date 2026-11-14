# #2185 : fiches AppStream des dépôts demandées à apt (50winlux-appstream, livré
# par winlux-desktop) et lues par la Boutique ; comportement vérifié par
# test-store-appstream-listes.py (CI), y compris par apt-config.
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
_st = (ROOT / "scripts/stage-apt-root.py").read_text(encoding="utf-8")
ok((ROOT / "integration/apt/50winlux-appstream").is_file() and "50winlux-appstream" in _st,
   "winlux-desktop livre 50winlux-appstream (#2185)")
ok((ROOT / "scripts/test-store-appstream-listes.py").is_file() and "test-store-appstream-listes.py" in _ci,
   "Boutique : fiches AppStream des listes apt testées en CI (#2185)")
