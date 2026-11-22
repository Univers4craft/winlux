# #2831 : _ismbblead reconnaît les octets de tête de la page de code
# courante (0x81–0x9F et 0xE0–0xFC en 932) au lieu de rendre toujours 0.
# Comportement gardé par le test crt_extra::tests::ismbblead_suit_la_page_de_code.
extra_2831 = (ROOT / "runtime" / "src" / "dlls" / "crt_extra.rs").read_text(encoding="utf-8")
ok('"_ismbblead" => fn ismbblead' in extra_2831
   and "fn ismbblead_suit_la_page_de_code" in extra_2831,
   "CRT : _ismbblead suit la page de code (#2831)")
