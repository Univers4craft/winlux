# #1204 : le SAFEARRAY rendu a la disposition publique (cDims, pvData,
# rgsabound) et SafeArrayPutElement copie la chaîne d'un VT_BSTR.
ok("fn sync_header(&mut self)" in (runtime_dlls / "oleaut32.rs").read_text(encoding="utf-8")
   and "safearray_a_la_disposition_publique_et_copie_les_chaines" in (SCRIPTS / "test-all.d/1204-safearray.sh").read_text(encoding="utf-8"),
   "SAFEARRAY : structure publique de oaidl.h, BSTR copiées par Put/GetElement (#1204)")
