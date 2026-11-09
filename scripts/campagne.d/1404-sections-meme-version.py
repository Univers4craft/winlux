# #1404 : une nouvelle section n'efface pas les autres sections du même .exe.
ok("fn stale(file: &str, version: &str) -> bool" in (ROOT / "runtime/src/image_cache.rs").read_text(encoding="utf-8")
   and "section_alignee_creee_puis_reprise_et_ancienne_version_effacee" in (SCRIPTS / "test-all.d/1404-sections-meme-version.sh").read_text(encoding="utf-8"),
   "Cache d'images : sections d'une même version conservées (#1404)")
