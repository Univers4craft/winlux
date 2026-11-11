# #1388 : le code de claude.exe n'est plus recopié dans chaque processus.
ok("file.map_cached(path, base + d, start, n / PAGE * PAGE)" in (ROOT / "runtime/src/loader.rs").read_text(encoding="utf-8")
   and "fn account_home()" in (ROOT / "runtime/src/image_cache.rs").read_text(encoding="utf-8")
   and "section_alignee_creee_puis_reprise_et_ancienne_version_effacee" in (SCRIPTS / "test-all.d/1388-sections-en-cache.sh").read_text(encoding="utf-8"),
   "Sections mal alignées projetées depuis le cache partagé (#1388)")
