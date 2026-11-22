# #1304 : CheckFormatSupport déclare RENDER_TARGET, SHADER_SAMPLE… par format.
ok("format_support(" in (runtime_dlls / "d3d11.rs").read_text(encoding="utf-8")
   and "formats_de_rendu_annonces" in (SCRIPTS / "test-all.d" / "1304-checkformatsupport.sh").read_text(encoding="utf-8"),
   "CheckFormatSupport par format (#1304)")
