# #2796 : LoadRegTypeLib(SHDocVw 1.1) rend la dispinterface
# DWebBrowserEvents2, et un programme x86 peut lire ITypeLib, ITypeInfo,
# TYPEATTR et FUNCDESC (installeur Creative Cloud, puits d'événements ATL
# du navigateur intégré). Comportements gardés par les tests
# dlls::oleaut32_typelib::tests::shdocvw_dwebbrowserevents2_par_loadregtypelib_2796
# et wow32::com_typeinfo::tests::funcdesc_et_typeattr_en_disposition_x86_2796.
tl_2796 = (ROOT / "runtime" / "src" / "dlls" / "oleaut32_typelib.rs").read_text(encoding="utf-8")
ti_2796 = (ROOT / "runtime" / "src" / "wow32" / "com_typeinfo.rs").read_text(encoding="utf-8")
ok("fn shdocvw_dwebbrowserevents2_par_loadregtypelib_2796" in tl_2796,
   "SHDocVw 1.1 : DWebBrowserEvents2 rendue par LoadRegTypeLib (#2796)")
ok("fn funcdesc_et_typeattr_en_disposition_x86_2796" in ti_2796,
   "TYPEATTR et FUNCDESC traduits pour un programme x86 (#2796)")
