# #2139 : versions non sécurisées tmpfile, _rmtmp, _gcvt, _fcvt, _searchenv
# et _wsearchenv servies. Comportement gardé par les tests
# dlls::crt_secure_reste::tests::{tmpfile_et_rmtmp, gcvt_et_fcvt_non_securises,
# searchenv_non_securise}.
rest_2139 = (ROOT / "runtime" / "src" / "dlls" / "crt_secure_reste.rs").read_text(encoding="utf-8")
tests_2139 = (ROOT / "runtime" / "src" / "dlls" / "crt_secure_reste" / "tests.rs").read_text(encoding="utf-8")
ok('"tmpfile" => fn tmpfile, "_rmtmp" => fn rmtmp' in rest_2139
   and '"_searchenv" => fn searchenv_ns' in rest_2139
   and "fn tmpfile_et_rmtmp" in tests_2139,
   "CRT : tmpfile, _rmtmp, _gcvt, _fcvt et _searchenv servis (#2139)")
