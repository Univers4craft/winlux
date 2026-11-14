# Auto-extractible 7-Zip qui lance son propre programme (Zoom) : le programme de RunProgram/ExecuteFile est lancé (#2390).
_s = (ROOT / "runtime/src/setup/sevenz.rs").read_text(encoding="utf-8")
ok("fn run_sfx_program(" in _s and "fn programme_du_module_sfx(" in _s
   and "scripts/test-sfx-programme.sh" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
   "auto-extractible 7-Zip : programme du module SFX lancé et attendu, son code rendu (#2390)")
