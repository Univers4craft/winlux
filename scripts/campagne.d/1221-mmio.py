# winmm!mmioOpenW et la famille mmio* (RIFF, fichiers en mémoire).
ok('"mmioOpenW"' in (runtime_dlls / "winmm_mmio.rs").read_text(encoding="utf-8")
   and "winmm_mmio::exports()" in (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
   and "mmio_fichier_en_memoire" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "winmm : mmioOpenW, mmioDescend/Ascend/CreateChunk, fichiers en mémoire (#1221)")
