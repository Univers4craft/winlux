# Installateurs décompressés dans le %TEMP% du profil, sur le disque, pas dans /tmp en mémoire (#2326).
_s = "\n".join((ROOT / f"runtime/src/{f}").read_text(encoding="utf-8")
               for f in ("setup/sevenz.rs", "setup/burn.rs", "setup/web.rs", "setup/nsis.rs", "msi/actions.rs"))
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_p = (ROOT / "runtime/src/paths.rs").read_text(encoding="utf-8")
# Test unitaire dossier_temporaire_du_profil_sur_le_disque : setup_work_dir sous %LOCALAPPDATA%\\Temp.
_f = {f: (ROOT / f"runtime/src/{f}").read_text(encoding="utf-8") for f in ("setup/sevenz.rs", "setup/burn.rs", "msi/actions.rs")}
ok("pub fn setup_work_dir(" in _p and all("paths::setup_work_dir" in t for t in _f.values())
   and "fn dossier_temporaire_du_profil_sur_le_disque(" in (ROOT / "runtime/src/paths/tests.rs").read_text(encoding="utf-8"),
   "installateurs : archive décompressée dans le dossier temporaire du profil, sur le disque (#2326)")
ok("paths::profile_temp" in _k and "pub fn profile_temp(" in _p and '"/tmp"' not in _k.split("fn temp_path()")[1].split("\n}\n")[0],
   "GetTempPath : TMP, TEMP, sinon %LOCALAPPDATA%\\Temp, jamais /tmp par défaut (#2326)")
ok('("TEMP".into()' in _p and '("TMP".into()' in _p,
   "environnement Windows : TEMP et TMP désignent le dossier temporaire du profil (#2326)")
