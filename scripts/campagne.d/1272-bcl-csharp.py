# #1272 : étape 7 du moteur .NET, mscorlib écrite en C# et compilée par
# Roslyn épinglé ; le moteur la charge et l'attache aux types intégrés.
bcl_build = (SCRIPTS / "build-bcl.sh").read_text(encoding="utf-8")
clr_dir = runtime_dlls.parent / "clr"
ok("roslyn_sha256=e3f0207e7a8caf2cf79ad8f276eb20b2b880b3a7bb0e8ad27b4d3183d0a81627" in bcl_build
   and "sha256sum --check" in bcl_build and "-nostdlib" in bcl_build
   and (SCRIPTS.parent / "bcl/mscorlib/Noyau.cs").is_file()
   and "public static bool IsNullOrWhiteSpace" in (SCRIPTS.parent / "bcl/mscorlib/Noyau.cs").read_text(encoding="utf-8"),
   "BCL C# : Roslyn 5.0.0 épinglé (SHA-256), mscorlib compilée sans bibliothèque tierce (#1272)")
ok("pub const CORLIB" in (clr_dir / "domain.rs").read_text(encoding="utf-8")
   and "pub fn is_builtin" in (clr_dir / "types.rs").read_text(encoding="utf-8")
   and "notre_mscorlib_s_attache_aux_types_integres" in (clr_dir / "tests.rs").read_text(encoding="utf-8")
   and "/opt/winlux/bcl/mscorlib.dll" in (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
   and "test-bcl.sh" in (SCRIPTS / "test-all.d" / "1272-bcl-csharp.sh").read_text(encoding="utf-8"),
   "CLR : notre mscorlib chargée depuis /opt/winlux/bcl et attachée aux types intégrés, dans l'ISO (#1272)")
