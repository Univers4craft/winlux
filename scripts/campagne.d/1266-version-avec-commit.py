# #1266 : `winlux --version` donne aussi le commit de construction, sans quoi
# deux têtes de master affichaient toutes deux « v0.2.20 » sur le banc.
ok("WINLUX_BUILD_COMMIT" in (ROOT / "runtime/build.rs").read_text(encoding="utf-8")
   and "fn version_line" in (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8")
   and 'println!("{}", version_line());' in (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8"),
   "winlux --version affiche la version et le commit de construction (#1266)")
