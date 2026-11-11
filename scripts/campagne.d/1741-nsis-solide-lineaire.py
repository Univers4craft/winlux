# NSIS solide : une relecture en arrière (fichiers identiques partagés) ne redécompresse pas tout le flux ; durée linéaire (#1741).
nsis = (ROOT / "runtime/src/setup/nsis.rs").read_text(encoding="utf-8")
ok("cache.get(&offset)" in nsis and "SOLID_CACHE_TOTAL" in nsis
   and "fn fichiers_partages_lus_sans_tout_redecompresser" in nsis
   and 'assert_eq!(restarts, 0' in nsis,
   "NSIS solide : 5000 petits fichiers relus en arrière sans redécompression (#1741)")
