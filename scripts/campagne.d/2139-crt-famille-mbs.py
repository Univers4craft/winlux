# #2139 : fonctions de chaînes multioctets _mbslen, _mbsstr, _mbsicmp,
# _mbsncpy, _mbstok… et leurs variantes _l servies. Comportement gardé par
# les tests dlls::crt_mbs::tests (mbslen_mbsdec_en_page_932,
# mbsstr_mbsspn_mbsrev, mbsncpy_mbsncat_mbsnbcat, mbslwr_mbsset_mbstok).
mbs_2139 = (ROOT / "runtime" / "src" / "dlls" / "crt_mbs.rs").read_text(encoding="utf-8")
ok('"_mbslen" => fn mbslen' in mbs_2139
   and '"_mbsstr" => fn mbsstr' in mbs_2139
   and '"_mbstok" => fn mbstok' in mbs_2139,
   "CRT : fonctions de chaînes multioctets _mbs* servies (#2139)")
