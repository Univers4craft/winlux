# #2685 : CryptQueryObject lit la signature PKCS #7 incorporée à un PE
# (greffons d'Acrobat). Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/crypt32.rs").read_text(encoding="utf-8")
ok("fn signature_incorporee_d_un_greffon_lue" in t, "crypt32 : signature incorporée d'un greffon lue par CryptQueryObject (#2685)")
