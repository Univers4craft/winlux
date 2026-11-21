# #2137 : variantes _l des fonctions sécurisées de la CRT (_printf_s_l,
# _sscanf_s_l, _strlwr_s_l, _mbstowcs_s_l…) servies, avec leur passerelle
# x86. Comportement gardé par les tests
# dlls::crt_secure::tests::variantes_l_comme_sans_locale et
# wow32::crt::tests::variantes_l_securisees_ont_leur_passerelle.
sec_2137 = (ROOT / "runtime" / "src" / "dlls" / "crt_secure.rs").read_text(encoding="utf-8")
w32_2137 = (ROOT / "runtime" / "src" / "wow32" / "crt.rs").read_text(encoding="utf-8")
ok('"_sprintf_s_l" => fn sprintf_s_l_thunk' in sec_2137
   and '"_mbstowcs_s_l" => fn mbstowcs_s_l' in sec_2137
   and "fn variantes_l_comme_sans_locale" in sec_2137
   and "fn variantes_l_securisees_ont_leur_passerelle" in w32_2137,
   "CRT : variantes _l des fonctions sécurisées servies (#2137)")
